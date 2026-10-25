use crate::{
    color::Palette,
    fs::{Entry, Kind, Timestamp, permissions},
    options::{Compat, Format, Options, Quote, When},
    quote,
};
use std::{
    collections::HashMap,
    ffi::CStr,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
};
use unicode_width::UnicodeWidthStr;

struct Cell {
    bytes: Vec<u8>,
    width: usize,
}
impl Cell {
    fn native(bytes: Vec<u8>, compat: Compat) -> Self {
        let width = quote::display_width(&bytes, compat);
        Self { bytes, width }
    }
}
impl From<String> for Cell {
    fn from(text: String) -> Self {
        let width = UnicodeWidthStr::width(text.as_str());
        Self {
            bytes: text.into_bytes(),
            width,
        }
    }
}

#[derive(Default)]
struct Names {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}
impl Names {
    fn user(&mut self, id: u32, numeric: bool) -> String {
        if numeric {
            return id.to_string();
        }
        self.users
            .entry(id)
            .or_insert_with(|| lookup(id, true))
            .clone()
    }
    fn group(&mut self, id: u32, numeric: bool) -> String {
        if numeric {
            return id.to_string();
        }
        self.groups
            .entry(id)
            .or_insert_with(|| lookup(id, false))
            .clone()
    }
}

fn lookup(id: u32, user: bool) -> String {
    let mut size = 4096;
    while size <= 1024 * 1024 {
        let mut buffer = vec![0u8; size];
        let (status, name) = unsafe {
            if user {
                let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
                let mut result = std::ptr::null_mut();
                let status = libc::getpwuid_r(
                    id,
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    size,
                    &mut result,
                );
                (
                    status,
                    if status == 0 && !result.is_null() {
                        Some(
                            CStr::from_ptr((*result).pw_name)
                                .to_string_lossy()
                                .into_owned(),
                        )
                    } else {
                        None
                    },
                )
            } else {
                let mut entry = std::mem::MaybeUninit::<libc::group>::uninit();
                let mut result = std::ptr::null_mut();
                let status = libc::getgrgid_r(
                    id,
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    size,
                    &mut result,
                );
                (
                    status,
                    if status == 0 && !result.is_null() {
                        Some(
                            CStr::from_ptr((*result).gr_name)
                                .to_string_lossy()
                                .into_owned(),
                        )
                    } else {
                        None
                    },
                )
            }
        };
        if status == libc::ERANGE {
            size *= 2;
            continue;
        }
        return name.unwrap_or_else(|| id.to_string());
    }
    id.to_string()
}

pub struct Renderer<'a, W: Write> {
    pub out: W,
    pub o: &'a Options,
    pub palette: Palette,
    names: Names,
    offset: usize,
    dired_files: Vec<(usize, usize)>,
    dired_dirs: Vec<(usize, usize)>,
    dates: HashMap<(i64, i64), Vec<u8>>,
}
impl<'a, W: Write> Renderer<'a, W> {
    pub fn new(out: W, o: &'a Options, mut palette: Palette) -> Self {
        if o.dired {
            palette.enabled = false;
        }
        Self {
            out,
            o,
            palette,
            names: Names::default(),
            offset: 0,
            dired_files: vec![],
            dired_dirs: vec![],
            dates: HashMap::new(),
        }
    }
    pub fn emit(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)?;
        self.offset += bytes.len();
        Ok(())
    }
    pub fn newline(&mut self) -> io::Result<()> {
        self.emit(if self.o.zero { b"\0" } else { b"\n" })
    }
    pub fn header(&mut self, path: &Path, blank: bool) -> io::Result<()> {
        if blank {
            self.newline()?;
        }
        if self.o.dired {
            self.emit(b"  ")?;
        }
        let bytes = quote::name(path.as_os_str(), self.o.quote);
        let start = self.offset;
        let colored = self.palette.wrap(&self.palette.colors.header, &bytes);
        self.emit(&colored)?;
        if self.o.dired {
            self.dired_dirs.push((start, self.offset));
        }
        self.emit(b":")?;
        self.newline()
    }
    fn name(&self, e: &Entry, link: bool) -> Cell {
        let mut raw = quote::name(&e.name, self.o.quote);
        if self.o.icons {
            let icon = match e.kind {
                Kind::Directory => "▸ ",
                Kind::Symlink => "↗ ",
                Kind::Socket => "◉ ",
                Kind::Fifo => "│ ",
                _ => "· ",
            };
            let mut prefixed = icon.as_bytes().to_vec();
            prefixed.extend_from_slice(&raw);
            raw = prefixed;
        }
        raw.extend_from_slice(e.indicator(self.o.indicator).as_bytes());
        let width = quote::display_width(&raw, self.o.compat);
        let mut bytes = self.palette.file(e, &raw);
        if !self.o.dired
            && (self.o.hyperlink == When::Always || self.o.hyperlink == When::Auto && self.o.tty)
        {
            let mut linked = format!("\x1b]8;;{}\x1b\\", file_url(&e.path)).into_bytes();
            linked.extend_from_slice(&bytes);
            linked.extend_from_slice(b"\x1b]8;;\x1b\\");
            bytes = linked;
        }
        let mut cell = Cell { bytes, width };
        if link && let Some(target) = &e.link {
            let raw = quote::name(target.as_os_str(), self.o.quote);
            cell.width += 4 + UnicodeWidthStr::width(String::from_utf8_lossy(&raw).as_ref());
            cell.bytes.extend_from_slice(b" -> ");
            cell.bytes
                .extend_from_slice(&self.palette.wrap(&self.palette.colors.symlink, &raw));
        }
        cell
    }
    pub fn listing(&mut self, entries: &[Entry], directory: bool) -> io::Result<()> {
        match self.o.format {
            Format::Long | Format::Table => self.long(entries, directory),
            Format::Columns | Format::Across => self.columns(entries),
            Format::Commas => self.commas(entries),
            _ => self.single(entries),
        }
    }
    fn prefix(&self, e: &Entry, widths: (usize, usize, usize)) -> Vec<u8> {
        let mut prefix = String::new();
        if self.o.inode {
            prefix.push_str(&format!("{:>width$} ", e.inode, width = widths.0));
        }
        if self.o.blocks {
            prefix.push_str(&format!(
                "{:>width$} ",
                self.blocks(e.stat.as_ref().map_or(0, |s| s.blocks)),
                width = widths.1
            ));
        }
        if self.o.context {
            prefix.push_str(&format!(
                "{:<width$} ",
                e.context.as_deref().unwrap_or("?"),
                width = widths.2
            ));
        }
        prefix.into_bytes()
    }
    fn prefix_widths(&self, entries: &[Entry]) -> (usize, usize, usize) {
        (
            if self.o.inode {
                entries
                    .iter()
                    .map(|e| e.inode.to_string().len())
                    .max()
                    .unwrap_or(1)
                    .max(self.o.column_minima[0])
            } else {
                0
            },
            if self.o.blocks {
                entries
                    .iter()
                    .map(|e| self.blocks(e.stat.as_ref().map_or(0, |s| s.blocks)).len())
                    .max()
                    .unwrap_or(1)
                    .max(self.o.column_minima[1])
            } else {
                0
            },
            if self.o.context {
                entries
                    .iter()
                    .map(|e| e.context.as_deref().unwrap_or("?").len())
                    .max()
                    .unwrap_or(1)
            } else {
                0
            },
        )
    }
    fn blocks(&self, blocks: u64) -> String {
        let size = blocks.saturating_mul(512);
        if self.o.human && self.o.compat == Compat::Gnu {
            human(size, self.o.si)
        } else {
            size.div_ceil(self.o.block_size).to_string()
        }
    }
    pub fn total(&mut self, entries: &[Entry]) -> io::Result<()> {
        if self.o.dired {
            self.emit(b"  ")?;
        }
        let total = entries
            .iter()
            .filter_map(|e| e.stat.as_ref())
            .map(|s| s.blocks)
            .sum();
        self.emit(format!("total {}", self.blocks(total)).as_bytes())?;
        self.newline()
    }
    fn single(&mut self, entries: &[Entry]) -> io::Result<()> {
        if !self.o.inode
            && !self.o.blocks
            && !self.o.context
            && !self.o.icons
            && !self.palette.enabled
            && self.o.hyperlink == When::Never
            && self.o.quote == Quote::Literal
        {
            for e in entries {
                self.emit(e.name.as_bytes())?;
                self.emit(e.indicator(self.o.indicator).as_bytes())?;
                self.newline()?;
            }
            return Ok(());
        }
        let widths = self.prefix_widths(entries);
        for e in entries {
            self.emit(&self.prefix(e, widths))?;
            self.emit(&self.name(e, false).bytes)?;
            self.newline()?;
        }
        Ok(())
    }
    fn short_cells(&self, entries: &[Entry]) -> Vec<Cell> {
        let widths = self.prefix_widths(entries);
        entries
            .iter()
            .map(|e| {
                let prefix = self.prefix(e, widths);
                let cell = self.name(e, false);
                let width = prefix.len() + cell.width;
                let mut bytes = prefix;
                bytes.extend(cell.bytes);
                Cell { bytes, width }
            })
            .collect()
    }
    fn columns(&mut self, entries: &[Entry]) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let cells = self.short_cells(entries);
        if self.o.compat == Compat::Bsd {
            return self.bsd_columns(entries, &cells);
        }
        let width = self.o.width;
        if width == 0 {
            for (i, cell) in cells.iter().enumerate() {
                if i > 0 {
                    self.emit(b"  ")?;
                }
                self.emit(&cell.bytes)?;
            }
            return self.newline();
        }
        // O(n * terminal_width), bounded by the minimum cell width; narrow
        // terminals never trigger quadratic work on a huge directory.
        let minimum = cells.iter().map(|c| c.width).min().unwrap_or(1).max(1);
        let max_cols = (width / (minimum + 2)).max(1).min(cells.len());
        let mut layout = (
            cells.len(),
            1,
            vec![cells.iter().map(|c| c.width).max().unwrap_or(0)],
        );
        for cols in (1..=max_cols).rev() {
            let rows = cells.len().div_ceil(cols);
            let mut widths = vec![0usize; cols];
            for (i, cell) in cells.iter().enumerate() {
                let col = if self.o.format == Format::Across {
                    i % cols
                } else {
                    i / rows
                };
                widths[col] = widths[col].max(cell.width);
            }
            if widths.iter().sum::<usize>() + (cols - 1) * 2 <= width {
                layout = (rows, cols, widths);
                break;
            }
        }
        let (rows, cols, widths) = layout;
        for row in 0..rows {
            let mut position = 0;
            for (col, col_width) in widths.iter().enumerate().take(cols) {
                let index = if self.o.format == Format::Across {
                    row * cols + col
                } else {
                    col * rows + row
                };
                let Some(cell) = cells.get(index) else {
                    continue;
                };
                self.emit(&cell.bytes)?;
                position += cell.width;
                let next = if self.o.format == Format::Across {
                    row * cols + col + 1
                } else {
                    (col + 1) * rows + row
                };
                if col + 1 < cols && next < cells.len() {
                    let gap = col_width.saturating_sub(cell.width) + 2;
                    self.padding(position, gap)?;
                    position += gap;
                }
            }
            self.newline()?;
        }
        Ok(())
    }
    fn bsd_columns(&mut self, entries: &[Entry], cells: &[Cell]) -> io::Result<()> {
        let tab = if self.palette.enabled || self.o.tabsize == 0 {
            1
        } else {
            8
        };
        let prefixes = self.prefix_widths(entries);
        let max_name = entries
            .iter()
            .map(|e| quote::display_width(&quote::name(&e.name, self.o.quote), self.o.compat))
            .max()
            .unwrap_or(0)
            .max(self.o.column_minima[7]);
        let max = max_name
            + if self.o.inode { prefixes.0 + 1 } else { 0 }
            + if self.o.blocks { prefixes.1 + 1 } else { 0 }
            + if self.o.indicator != crate::options::Indicator::None {
                1
            } else {
                0
            }
            + if self.o.context { prefixes.2 + 1 } else { 0 }
            + if self.o.icons { 2 } else { 0 };
        let column = (max / tab + 1) * tab;
        let cols = (self.o.width / column.max(1)).min(cells.len());
        if cols < 2 {
            return self.single(entries);
        }
        let rows = cells.len().div_ceil(cols);
        for row in 0..rows {
            let mut position = 0;
            for col in 0..cols {
                let i = if self.o.format == Format::Across {
                    row * cols + col
                } else {
                    col * rows + row
                };
                let Some(cell) = cells.get(i) else {
                    break;
                };
                self.emit(&cell.bytes)?;
                position += cell.width;
                let next = if self.o.format == Format::Across {
                    i + 1
                } else {
                    i + rows
                };
                if col + 1 < cols && next < cells.len() {
                    let target = (col + 1) * column;
                    while position < target {
                        if tab == 8 {
                            self.emit(b"\t")?;
                            position = (position / 8 + 1) * 8;
                        } else {
                            self.emit(b" ")?;
                            position += 1;
                        }
                    }
                }
            }
            self.newline()?;
        }
        Ok(())
    }
    fn padding(&mut self, position: usize, gap: usize) -> io::Result<()> {
        let target = position + gap;
        let mut at = position;
        while self.o.tabsize > 0 {
            let next = (at / self.o.tabsize + 1) * self.o.tabsize;
            if next > target || next - at < 2 {
                break;
            }
            self.emit(b"\t")?;
            at = next;
        }
        self.emit(&vec![b' '; target - at])
    }
    fn commas(&mut self, entries: &[Entry]) -> io::Result<()> {
        let cells = self.short_cells(entries);
        if self.o.compat == Compat::Bsd {
            let mut column = 0;
            for (index, (entry, cell)) in entries.iter().zip(&cells).enumerate() {
                let separator = usize::from(index + 1 < entries.len()) * 2;
                if column + entry.name.as_bytes().len() + separator >= self.o.width {
                    self.newline()?;
                    column = 0;
                }
                self.emit(&cell.bytes)?;
                column += cell.width;
                if separator > 0 {
                    self.emit(b", ")?;
                    column += 2;
                }
            }
            if column > 0 {
                self.newline()?;
            }
            return Ok(());
        }
        let mut column = 0;
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                self.emit(b",")?;
                column += 1;
                if self.o.width > 0 && column + 1 + cell.width > self.o.width {
                    self.newline()?;
                    column = 0;
                } else {
                    self.emit(b" ")?;
                    column += 1;
                }
            }
            self.emit(&cell.bytes)?;
            column += cell.width;
        }
        if !cells.is_empty() {
            self.newline()?;
        }
        Ok(())
    }
    fn long(&mut self, entries: &[Entry], directory: bool) -> io::Result<()> {
        let table = self.o.format == Format::Table;
        if directory && !table {
            self.total(entries)?;
        }
        let marker = self.o.compat == Compat::Bsd
            || entries
                .iter()
                .any(|e| e.acl.is_some() || e.context.is_some());
        let mut headers = vec![];
        let mut numeric = vec![];
        let mut add = |s: &str, n: bool| {
            headers.push(s.to_owned());
            numeric.push(n);
        };
        if self.o.inode {
            add("Inode", true);
        }
        if self.o.blocks {
            add("Blocks", true);
        }
        add("Permissions", false);
        add("Links", true);
        if self.o.owner {
            add("Owner", false);
        }
        if self.o.group {
            add("Group", false);
        }
        if self.o.author {
            add("Author", false);
        }
        if self.o.flags {
            add("Flags", false);
        }
        if self.o.context {
            add("Context", false);
        }
        add("Size", true);
        add("Modified", false);
        let mut rows: Vec<Vec<Cell>> = vec![];
        for e in entries {
            let mut row = vec![];
            if self.o.inode {
                row.push(e.inode.to_string().into());
            }
            if self.o.blocks {
                row.push(self.blocks(e.stat.as_ref().map_or(0, |s| s.blocks)).into());
            }
            let mut perm = permissions(e);
            if marker {
                perm.push(if self.o.dataless && e.dataless() {
                    '%'
                } else if self.o.compat == Compat::Bsd && e.has_xattrs {
                    '@'
                } else if e.acl.is_some() {
                    '+'
                } else if e.context.is_some() {
                    '.'
                } else {
                    ' '
                });
            }
            row.push(perm.into());
            if let Some(s) = &e.stat {
                row.push(s.links.to_string().into());
                if self.o.owner {
                    row.push(self.names.user(s.uid, self.o.numeric).into());
                }
                if self.o.group {
                    row.push(self.names.group(s.gid, self.o.numeric).into());
                }
                if self.o.author {
                    row.push(self.names.user(s.uid, self.o.numeric).into());
                }
                if self.o.flags {
                    row.push(crate::platform::flags(s.flags).into());
                }
                if self.o.context {
                    row.push(e.context.clone().unwrap_or_else(|| "?".into()).into());
                }
                row.push(if matches!(e.kind, Kind::CharDevice | Kind::BlockDevice) {
                    if self.o.compat == Compat::Bsd {
                        format!("{:#x}", s.rdev).into()
                    } else {
                        let (a, b) = crate::platform::device_parts(s.rdev);
                        format!("{a}, {b}").into()
                    }
                } else {
                    self.size(s.size)
                });
                let date = self.date(e.stamp(self.o.time));
                row.push(Cell::native(date, self.o.compat));
            } else {
                while row.len() < headers.len() {
                    row.push(String::from("?").into());
                }
            }
            rows.push(row);
        }
        let mut widths = vec![0; headers.len()];
        for row in &rows {
            for (i, value) in row.iter().enumerate() {
                widths[i] = widths[i].max(value.width);
            }
        }
        if self.o.compat == Compat::Bsd && !table {
            let mut column = 0;
            for (enabled, minimum) in [
                (self.o.inode, Some(0)),
                (self.o.blocks, Some(1)),
                (true, None),
                (true, Some(2)),
                (self.o.owner, Some(3)),
                (self.o.group, Some(4)),
                (self.o.author, Some(3)),
                (self.o.flags, Some(5)),
                (self.o.context, None),
                (true, Some(6)),
                (true, None),
            ] {
                if enabled {
                    if let Some(index) = minimum {
                        widths[column] = widths[column].max(self.o.column_minima[index]);
                    }
                    column += 1;
                }
            }
            if self.o.thousands && !self.o.human {
                let raw = entries
                    .iter()
                    .filter_map(|e| e.stat.as_ref())
                    .map(|s| s.size.to_string().len())
                    .max()
                    .unwrap_or(1);
                widths[headers.len() - 2] = (raw + raw / 3).max(self.o.column_minima[6]);
            }
        }
        if self.o.human && self.o.compat == Compat::Bsd && !table {
            widths[headers.len() - 2] = widths[headers.len() - 2].max(5);
        }
        if table {
            for (i, h) in headers.iter().enumerate() {
                widths[i] = widths[i].max(h.len());
            }
        }
        if table {
            let headers: Vec<Cell> = headers.iter().cloned().map(Cell::from).collect();
            self.fields(&headers, &widths, &numeric, true)?;
            let header = self.palette.wrap(&self.palette.colors.header, b"Name");
            self.emit(&header)?;
            self.newline()?;
            let line = "─".repeat(
                (widths.iter().sum::<usize>() + widths.len() * 2 + 24).min(self.o.width.max(24)),
            );
            self.emit(
                &self
                    .palette
                    .wrap(&self.palette.colors.muted, line.as_bytes()),
            )?;
            self.newline()?;
        }
        for (e, row) in entries.iter().zip(rows) {
            if self.o.dired {
                self.emit(b"  ")?;
            }
            self.fields(&row, &widths, &numeric, false)?;
            let start = self.offset;
            self.emit(&self.name(e, false).bytes)?;
            if self.o.dired {
                self.dired_files.push((start, self.offset));
            }
            if let Some(target) = &e.link {
                self.emit(b" -> ")?;
                self.emit(&quote::name(target.as_os_str(), self.o.quote))?;
            }
            self.newline()?;
            if self.o.xattrs {
                for (name, size) in &e.xattrs {
                    self.emit(b"\t")?;
                    self.emit(&quote::name(name, self.o.quote))?;
                    let size = self.size(*size);
                    self.emit(b"\t")?;
                    self.emit(&vec![
                        b' ';
                        widths[headers.len() - 2]
                            .saturating_sub(size.bytes.len())
                    ])?;
                    self.emit(&size.bytes)?;
                    self.emit(b" ")?;
                    self.newline()?;
                }
            }
            if self.o.acl
                && let Some(acl) = &e.acl
            {
                for (i, line) in acl.lines().enumerate() {
                    self.emit(format!(" {i}: {line}").as_bytes())?;
                    self.newline()?;
                }
            }
        }
        Ok(())
    }
    fn fields(
        &mut self,
        fields: &[Cell],
        widths: &[usize],
        numeric: &[bool],
        header: bool,
    ) -> io::Result<()> {
        for (i, field) in fields.iter().enumerate() {
            let field_width =
                if self.o.compat == Compat::Bsd && self.o.thousands && i + 2 == fields.len() {
                    field.bytes.len()
                } else {
                    field.width
                };
            let pad = widths[i].saturating_sub(field_width);
            let pad = if self.o.compat == Compat::Bsd
                && self.o.format != Format::Table
                && i + 1 == fields.len()
            {
                0
            } else {
                pad
            };
            if numeric[i] {
                self.emit(&vec![b' '; pad])?;
            }
            let code = if header {
                &self.palette.colors.header
            } else if self.o.format == Format::Table && i == fields.len() - 1 {
                &self.palette.colors.date
            } else {
                ""
            };
            self.emit(&self.palette.wrap(code, &field.bytes))?;
            if !numeric[i] {
                self.emit(&vec![b' '; pad])?;
            }
            // BSD's long form separates the identity columns with two spaces.
            let owner_start = usize::from(self.o.inode) + usize::from(self.o.blocks) + 2;
            let identities =
                usize::from(self.o.owner) + usize::from(self.o.group) + usize::from(self.o.author);
            self.emit(
                if self.o.compat == Compat::Bsd
                    && !self.o.owner
                    && !self.o.group
                    && i == usize::from(self.o.inode) + usize::from(self.o.blocks) + 1
                {
                    b"   "
                } else if self.o.format == Format::Table
                    || self.o.compat == Compat::Bsd
                        && (owner_start..owner_start + identities).contains(&i)
                {
                    b"  "
                } else {
                    b" "
                },
            )?;
        }
        Ok(())
    }
    fn size(&self, n: u64) -> Cell {
        if self.o.human {
            return (if self.o.compat == Compat::Bsd && !self.o.si {
                human_bsd(n)
            } else {
                human(n, self.o.si)
            })
            .into();
        }
        let n = if let Some(divisor) = self.o.size_divisor {
            n.div_ceil(divisor)
        } else {
            n
        };
        if self.o.thousands {
            Cell::native(grouped(n), self.o.compat)
        } else {
            n.to_string().into()
        }
    }
    fn date(&mut self, stamp: Option<Timestamp>) -> Vec<u8> {
        let Some(stamp) = stamp else {
            return b"-".to_vec();
        };
        let key = (
            stamp.seconds,
            if self.o.time_style.contains('N') || self.o.time_style == "full-iso" {
                stamp.nanoseconds
            } else {
                0
            },
        );
        if let Some(cached) = self.dates.get(&key) {
            return cached.clone();
        }
        let text = time(Some(stamp), &self.o.time_style, self.o.compat);
        if self.dates.len() < 4096 {
            self.dates.insert(key, text.clone());
        }
        text
    }
    pub fn tree_root(&mut self, e: &Entry) -> io::Result<()> {
        self.emit(&self.name(e, true).bytes)?;
        self.newline()
    }
    pub fn tree_entry(
        &mut self,
        e: &Entry,
        prefix: &str,
        last: bool,
        annotation: Option<&str>,
    ) -> io::Result<()> {
        let branch = format!("{prefix}{}", if last { "└── " } else { "├── " });
        self.emit(
            &self
                .palette
                .wrap(&self.palette.colors.muted, branch.as_bytes()),
        )?;
        self.emit(&self.name(e, true).bytes)?;
        if let Some(note) = annotation {
            self.emit(
                &self
                    .palette
                    .wrap(&self.palette.colors.muted, format!(" [{note}]").as_bytes()),
            )?;
        }
        self.newline()
    }
    pub fn finish(&mut self) -> io::Result<()> {
        if self.o.dired && self.o.format == Format::Long {
            let mut files = String::from("//DIRED//");
            for (a, b) in &self.dired_files {
                files.push_str(&format!(" {a} {b}"));
            }
            self.emit(files.as_bytes())?;
            self.newline()?;
            if !self.dired_dirs.is_empty() {
                let mut dirs = String::from("//SUBDIRED//");
                for (a, b) in &self.dired_dirs {
                    dirs.push_str(&format!(" {a} {b}"));
                }
                self.emit(dirs.as_bytes())?;
                self.newline()?;
            }
            let style = match self.o.quote {
                Quote::Literal => "literal",
                Quote::Escape => "escape",
                Quote::C => "c",
                Quote::Shell => "shell",
                Quote::ShellAlways => "shell-always",
                Quote::ShellEscape => "shell-escape",
                Quote::ShellEscapeAlways => "shell-escape-always",
                _ => "literal",
            };
            self.emit(format!("//DIRED-OPTIONS// --quoting-style={style}\n").as_bytes())?;
        }
        self.out.flush()
    }
}

pub fn human(n: u64, si: bool) -> String {
    let base = if si { 1000.0 } else { 1024.0 };
    let mut value = n as f64;
    let mut index = 0;
    while value >= base && index < 8 {
        value /= base;
        index += 1;
    }
    if index == 0 {
        return n.to_string();
    }
    let suffix = if si {
        ["", "k", "M", "G", "T", "P", "E", "Z", "Y"]
    } else {
        ["", "K", "M", "G", "T", "P", "E", "Z", "Y"]
    };
    let text = if value < 10.0 {
        format!("{:.1}{}", (value * 10.0).ceil() / 10.0, suffix[index])
    } else {
        format!("{:.0}{}", value.ceil(), suffix[index])
    };
    local_decimal(text)
}

fn human_bsd(n: u64) -> String {
    let mut value = n as f64;
    let mut index = 0;
    while value >= 999.5 && index < 6 {
        value /= 1024.0;
        index += 1;
    }
    let suffix = ["B", "K", "M", "G", "T", "P", "E"][index];
    let text = if index == 0 {
        format!("{n}B")
    } else if value < 9.95 {
        format!("{value:.1}{suffix}")
    } else {
        format!("{value:.0}{suffix}")
    };
    local_decimal(text)
}

fn local_decimal(text: String) -> String {
    let locale = unsafe { libc::localeconv() };
    if locale.is_null() {
        return text;
    }
    let point = unsafe { CStr::from_ptr((*locale).decimal_point) }.to_string_lossy();
    if point.as_ref() == "." {
        text
    } else {
        text.replace('.', point.as_ref())
    }
}

fn grouped(n: u64) -> Vec<u8> {
    let mut buffer = [0u8; 128];
    let length = unsafe {
        libc::snprintf(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            c"%'llu".as_ptr(),
            n as libc::c_ulonglong,
        )
    };
    if length < 0 || length as usize >= buffer.len() {
        n.to_string().into_bytes()
    } else {
        buffer[..length as usize].to_vec()
    }
}

fn time(stamp: Option<Timestamp>, style: &str, compat: Compat) -> Vec<u8> {
    let Some(stamp) = stamp else {
        return b"-".to_vec();
    };
    let seconds = stamp.seconds as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    if unsafe { libc::localtime_r(&seconds, tm.as_mut_ptr()) }.is_null() {
        return stamp.seconds.to_string().into_bytes();
    }
    let tm = unsafe { tm.assume_init() };
    let now = unsafe { libc::time(std::ptr::null_mut()) } as i64;
    let recent = if compat == Compat::Bsd {
        stamp.seconds < now + 1 && stamp.seconds > now - 182 * 86400
    } else {
        stamp.seconds <= now + 3600 && stamp.seconds > now - 15_778_476
    };
    let style = if let Some(plain) = style.strip_prefix("posix-") {
        let current = unsafe { libc::setlocale(libc::LC_TIME, std::ptr::null()) };
        if !current.is_null()
            && matches!(
                unsafe { CStr::from_ptr(current) }.to_bytes(),
                b"C" | b"POSIX"
            )
        {
            "locale"
        } else {
            plain
        }
    } else {
        style
    };
    #[cfg(target_os = "macos")]
    let day_first = compat == Compat::Bsd
        && unsafe { *libc::nl_langinfo(libc::D_MD_ORDER) } == b'd' as libc::c_char;
    #[cfg(not(target_os = "macos"))]
    let day_first = false;
    let format = match style {
        "bsd-full" => {
            if day_first {
                "%e %b %H:%M:%S %Y"
            } else {
                "%b %e %H:%M:%S %Y"
            }
        }
        "full-iso" => "%Y-%m-%d %H:%M:%S.%N %z",
        "long-iso" => "%Y-%m-%d %H:%M",
        "iso" => {
            if recent {
                "%m-%d %H:%M"
            } else {
                "%Y-%m-%d"
            }
        }
        "locale" => {
            if recent {
                if day_first {
                    "%e %b %H:%M"
                } else {
                    "%b %e %H:%M"
                }
            } else if day_first {
                "%e %b  %Y"
            } else {
                "%b %e  %Y"
            }
        }
        other => other.strip_prefix('+').unwrap_or("%b %e %H:%M"),
    };
    let format = if compat == Compat::Gnu
        && let Some((old, new)) = format.split_once('\n')
    {
        if recent { new } else { old }
    } else {
        format
    };
    let format = if compat == Compat::Gnu || style == "full-iso" {
        format.replace("%N", &format!("{:09}", stamp.nanoseconds))
    } else {
        format.to_owned()
    };
    let format = if compat == Compat::Bsd {
        pad_month(&format, tm.tm_mon)
    } else {
        format.into_bytes()
    };
    let Ok(format) = std::ffi::CString::new(format) else {
        return b"?".to_vec();
    };
    if format.as_bytes().is_empty() {
        return Vec::new();
    }
    let mut capacity = 128;
    while capacity <= 65_536 {
        let mut buf = vec![0u8; capacity];
        let n = unsafe { libc::strftime(buf.as_mut_ptr().cast(), buf.len(), format.as_ptr(), &tm) };
        if n > 0 {
            buf.truncate(n);
            return buf;
        }
        capacity *= 2;
    }
    b"?".to_vec()
}

fn pad_month(format: &str, month: i32) -> Vec<u8> {
    let Some(index) = format.find("%b") else {
        return format.as_bytes().to_vec();
    };
    let months: Vec<Vec<u8>> = (0..12)
        .map(|i| {
            unsafe { CStr::from_ptr(libc::nl_langinfo(libc::ABMON_1 + i)) }
                .to_bytes()
                .to_vec()
        })
        .collect();
    let max = months
        .iter()
        .map(|s| quote::display_width(s, Compat::Bsd))
        .max()
        .unwrap_or(0);
    let name = &months[month.clamp(0, 11) as usize];
    let padding = max.saturating_sub(quote::display_width(name, Compat::Bsd));
    let mut bytes = format.as_bytes()[..index].to_vec();
    bytes.extend_from_slice(name);
    bytes.extend(std::iter::repeat_n(b' ', padding));
    bytes.extend_from_slice(&format.as_bytes()[index + 2..]);
    bytes
}

fn file_url(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut result = "file://".to_owned();
    for b in absolute.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(b) {
            result.push(*b as char);
        } else {
            result.push_str(&format!("%{b:02X}"));
        }
    }
    result
}
