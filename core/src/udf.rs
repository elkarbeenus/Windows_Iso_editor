//! Minimal UDF reader + safe "append-only" patcher for Windows installation ISOs.
//!
//! Microsoft ISOs carry their real content in a UDF 1.02 file system (the ISO9660
//! part only contains a README). Instead of rebuilding the whole image (which can
//! break the BIOS/UEFI boot setup), we copy the original ISO bit for bit and then:
//!
//!   * append the new file(s) and a relocated copy of the root directory at the end,
//!   * rewrite the root File Entry so it points to the new directory,
//!   * enlarge the partition length (and update LVID / ISO9660 volume size),
//!   * write an extra Anchor Volume Descriptor Pointer at the new last sector.
//!
//! Boot catalog, boot images, `sources\install.wim` etc. stay untouched.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const SECTOR: u64 = 2048;

#[derive(Debug)]
pub struct UdfError(pub String);
impl std::fmt::Display for UdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for UdfError {}
impl From<std::io::Error> for UdfError {
    fn from(e: std::io::Error) -> Self {
        UdfError(format!("I/O: {e}"))
    }
}
pub type Result<T> = std::result::Result<T, UdfError>;

fn err<T>(s: impl Into<String>) -> Result<T> {
    Err(UdfError(s.into()))
}

// ---------------------------------------------------------------- helpers

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}
fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

/// CRC-ITU-T as required by ECMA-167 (poly 0x1021, init 0).
pub fn crc_itu(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn tag_checksum(t: &[u8]) -> u8 {
    let mut s: u8 = 0;
    for (i, &b) in t[..16].iter().enumerate() {
        if i != 4 {
            s = s.wrapping_add(b);
        }
    }
    s
}

pub fn tag_id(b: &[u8]) -> u16 {
    u16le(b, 0)
}

/// Checks tag checksum and (if the buffer is long enough) the CRC.
pub fn tag_valid(b: &[u8]) -> bool {
    if b.len() < 16 || tag_checksum(b) != b[4] {
        return false;
    }
    let crclen = u16le(b, 10) as usize;
    if 16 + crclen > b.len() {
        return false;
    }
    crc_itu(&b[16..16 + crclen]) == u16le(b, 8)
}

/// Fills in location, CRC (over `crc_len` bytes after the tag) and checksum.
fn finalize_tag(b: &mut [u8], location: u32, crc_len: usize) {
    put32(b, 12, location);
    put16(b, 10, crc_len as u16);
    let crc = crc_itu(&b[16..16 + crc_len]);
    put16(b, 8, crc);
    b[4] = tag_checksum(b);
}

/// Only the location changed (content and CRC stay valid).
fn relocate_tag(b: &mut [u8], location: u32) {
    put32(b, 12, location);
    b[4] = tag_checksum(b);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LongAd {
    pub len: u32,
    pub lbn: u32,
    pub part: u16,
}
impl LongAd {
    fn parse(b: &[u8], o: usize) -> Self {
        LongAd { len: u32le(b, o), lbn: u32le(b, o + 4), part: u16le(b, o + 8) }
    }
}

/// A run of data inside the partition (block number relative to partition start).
#[derive(Clone, Copy, Debug)]
struct Extent {
    lbn: u32,
    len: u32,
    recorded: bool,
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_parent: bool,
    pub characteristics: u8,
    pub icb: LongAd,
    /// raw FID bytes (including padding), as stored in the directory
    raw: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Node {
    lbn: u32,
    raw: Vec<u8>, // complete block containing the (Extended) File Entry
    extended: bool,
    info_len: u64,
    unique_id: u64,
    ea_len: usize,
    ad_off: usize, // offset of allocation descriptors inside raw
    extents: Vec<Extent>,
    inline: Option<Vec<u8>>,
    file_type: u8,
}

pub struct Udf {
    f: File,
    pub image_sectors: u64,
    pub block_size: u32,
    pub part_start: u32,
    pub part_len: u32,
    #[allow(dead_code)]
    part_num: u16,
    pd_sectors: Vec<u32>,
    lvid_sector: Option<u32>,
    avdp: Vec<u8>,
    root_icb: LongAd,
    pub volume_id: String,
}

fn decode_dstring_cs0(b: &[u8]) -> String {
    if b.is_empty() {
        return String::new();
    }
    let comp = b[0];
    let body = &b[1..];
    match comp {
        8 => body.iter().map(|&c| c as char).collect(),
        16 => {
            let mut v = Vec::new();
            for ch in body.chunks(2) {
                if ch.len() == 2 {
                    v.push(u16::from_be_bytes([ch[0], ch[1]]));
                }
            }
            String::from_utf16_lossy(&v)
        }
        _ => String::new(),
    }
}

/// Encodes a file name as OSTA CS0 (8-bit if possible, else 16-bit).
fn encode_cs0(name: &str) -> Vec<u8> {
    if name.chars().all(|c| (c as u32) < 256) {
        let mut v = vec![8u8];
        v.extend(name.chars().map(|c| c as u8));
        v
    } else {
        let mut v = vec![16u8];
        for u in name.encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        v
    }
}

impl Udf {
    pub fn open(path: &Path) -> Result<Self> {
        let f = File::options().read(true).open(path)?;
        Self::from_file(f)
    }

    pub fn open_rw(path: &Path) -> Result<Self> {
        let f = File::options().read(true).write(true).open(path)?;
        Self::from_file(f)
    }

    fn read_sector(&mut self, s: u64) -> Result<Vec<u8>> {
        read_sector(&mut self.f, s)
    }

    fn from_file(mut f: File) -> Result<Self> {
        let size = f.metadata()?.len();
        if size < 300 * SECTOR {
            return err("Datei ist zu klein für eine Windows-ISO / file too small");
        }
        let image_sectors = size.div_ceil(SECTOR);

        // Anchor at 256, fall back to last sector / last-256
        let mut avdp = None;
        for cand in [256u64, image_sectors - 1, image_sectors.saturating_sub(257)] {
            let b = read_sector(&mut f, cand)?;
            if tag_id(&b) == 2 && tag_valid(&b) {
                avdp = Some(b);
                break;
            }
        }
        let avdp = match avdp {
            Some(a) => a,
            None => return err("Kein UDF-Dateisystem gefunden (keine Windows-ISO?) / no UDF file system"),
        };
        let main_len = u32le(&avdp, 16);
        let main_loc = u32le(&avdp, 20);
        let res_len = u32le(&avdp, 24);
        let res_loc = u32le(&avdp, 28);

        let mut part: Option<(u32, u32, u16)> = None;
        let mut pd_sectors = Vec::new();
        let mut lvd: Option<Vec<u8>> = None;
        let mut volume_id = String::new();

        for (loc, len, is_main) in [(main_loc, main_len, true), (res_loc, res_len, false)] {
            let n = (len as u64 / SECTOR).min(64);
            for i in 0..n {
                let s = loc as u64 + i;
                if s >= image_sectors {
                    break;
                }
                let b = read_sector(&mut f, s)?;
                if !tag_valid(&b) {
                    continue;
                }
                match tag_id(&b) {
                    1 if is_main && volume_id.is_empty() => {
                        // Primary Volume Descriptor: volume identifier dstring(32) at 24
                        let l = b[24 + 31] as usize;
                        volume_id = decode_dstring_cs0(&b[24..24 + l.min(31)]);
                    }
                    5 => {
                        pd_sectors.push(s as u32);
                        if part.is_none() {
                            part = Some((u32le(&b, 188), u32le(&b, 192), u16le(&b, 22)));
                        }
                    }
                    6 if lvd.is_none() => lvd = Some(b),
                    8 => break,
                    _ => {}
                }
            }
        }
        let (part_start, part_len, part_num) = match part {
            Some(p) => p,
            None => return err("UDF: keine Partition gefunden / no partition descriptor"),
        };
        let lvd = match lvd {
            Some(l) => l,
            None => return err("UDF: kein Logical Volume Descriptor / no LVD"),
        };
        let block_size = u32le(&lvd, 212);
        if block_size as u64 != SECTOR {
            return err(format!("UDF: Blockgröße {block_size} wird nicht unterstützt"));
        }
        let n_maps = u32le(&lvd, 268);
        if n_maps != 1 || lvd[440] != 1 {
            return err("UDF: nur einfache Partitionen (Type 1) werden unterstützt / unsupported partition map");
        }
        let fsd_ad = LongAd::parse(&lvd, 248);
        let integ_len = u32le(&lvd, 432);
        let integ_loc = u32le(&lvd, 436);
        let lvid_sector = if integ_len > 0 && (integ_loc as u64) < image_sectors {
            let b = read_sector(&mut f, integ_loc as u64)?;
            if tag_id(&b) == 9 && tag_valid(&b) {
                Some(integ_loc)
            } else {
                None
            }
        } else {
            None
        };

        let fsd = read_sector(&mut f, part_start as u64 + fsd_ad.lbn as u64)?;
        if tag_id(&fsd) != 256 || !tag_valid(&fsd) {
            return err("UDF: File Set Descriptor ungültig");
        }
        let root_icb = LongAd::parse(&fsd, 400);

        Ok(Udf {
            f,
            image_sectors,
            block_size,
            part_start,
            part_len,
            part_num,
            pd_sectors,
            lvid_sector,
            avdp,
            root_icb,
            volume_id,
        })
    }

    fn read_node(&mut self, lbn: u32) -> Result<Node> {
        let raw = self.read_sector(self.part_start as u64 + lbn as u64)?;
        let id = tag_id(&raw);
        if (id != 261 && id != 266) || !tag_valid(&raw) {
            return err(format!("UDF: ungültiger File Entry bei Block {lbn}"));
        }
        let extended = id == 266;
        let file_type = raw[27];
        let flags = u16le(&raw, 34);
        let info_len = u64le(&raw, 56);
        let (unique_id, ea_len, ad_len, base) = if extended {
            (u64le(&raw, 200), u32le(&raw, 208) as usize, u32le(&raw, 212) as usize, 216)
        } else {
            (u64le(&raw, 160), u32le(&raw, 168) as usize, u32le(&raw, 172) as usize, 176)
        };
        let ad_off = base + ea_len;
        if ad_off + ad_len > raw.len() {
            return err("UDF: File Entry beschädigt");
        }
        let mut extents = Vec::new();
        let mut inline = None;
        match flags & 7 {
            0 | 1 => {
                let long = flags & 7 == 1;
                let step = if long { 16 } else { 8 };
                let mut ads: Vec<u8> = raw[ad_off..ad_off + ad_len].to_vec();
                let mut guard = 0;
                loop {
                    let mut next: Option<u32> = None;
                    let mut o = 0;
                    while o + step <= ads.len() {
                        let l = u32le(&ads, o);
                        let typ = l >> 30;
                        let len = l & 0x3FFF_FFFF;
                        if len == 0 {
                            break;
                        }
                        let pos = u32le(&ads, o + 4);
                        if typ == 3 {
                            next = Some(pos);
                            break;
                        }
                        extents.push(Extent { lbn: pos, len, recorded: typ == 0 });
                        o += step;
                    }
                    match next {
                        Some(n) if guard < 64 => {
                            guard += 1;
                            let b = self.read_sector(self.part_start as u64 + n as u64)?;
                            if tag_id(&b) != 258 {
                                return err("UDF: Allocation Extent ungültig");
                            }
                            let l = u32le(&b, 20) as usize;
                            ads = b[24..24 + l.min(b.len() - 24)].to_vec();
                        }
                        _ => break,
                    }
                }
            }
            3 => inline = Some(raw[ad_off..ad_off + ad_len].to_vec()),
            _ => return err("UDF: extended allocation descriptors nicht unterstützt"),
        }
        Ok(Node { lbn, raw, extended, info_len, unique_id, ea_len, ad_off, extents, inline, file_type })
    }

    fn read_node_data(&mut self, n: &Node, max: u64) -> Result<Vec<u8>> {
        if let Some(d) = &n.inline {
            return Ok(d[..(n.info_len as usize).min(d.len())].to_vec());
        }
        let want = n.info_len.min(max);
        let mut out = Vec::with_capacity(want as usize);
        for e in &n.extents {
            if out.len() as u64 >= want {
                break;
            }
            let take = (e.len as u64).min(want - out.len() as u64) as usize;
            if e.recorded {
                let mut buf = vec![0u8; take];
                self.f.seek(SeekFrom::Start((self.part_start as u64 + e.lbn as u64) * SECTOR))?;
                self.f.read_exact(&mut buf)?;
                out.extend_from_slice(&buf);
            } else {
                out.resize(out.len() + take, 0);
            }
        }
        Ok(out)
    }

    fn parse_dir(data: &[u8]) -> Vec<DirEntry> {
        let mut v = Vec::new();
        let mut o = 0;
        while o + 38 <= data.len() {
            if tag_id(&data[o..]) != 257 {
                break;
            }
            let chars = data[o + 18];
            let l_fi = data[o + 19] as usize;
            let icb = LongAd::parse(data, o + 20);
            let l_iu = u16le(data, o + 36) as usize;
            let total = (38 + l_iu + l_fi).div_ceil(4) * 4;
            if o + 38 + l_iu + l_fi > data.len() {
                break;
            }
            let name_raw = &data[o + 38 + l_iu..o + 38 + l_iu + l_fi];
            let end = (o + total).min(data.len());
            v.push(DirEntry {
                name: decode_dstring_cs0(name_raw),
                is_dir: chars & 0x02 != 0,
                is_parent: chars & 0x08 != 0,
                characteristics: chars,
                icb,
                raw: data[o..end].to_vec(),
            });
            o += total;
        }
        v
    }

    fn dir_entries(&mut self, icb: LongAd) -> Result<(Node, Vec<DirEntry>)> {
        let node = self.read_node(icb.lbn)?;
        let data = self.read_node_data(&node, 64 * 1024 * 1024)?;
        Ok((node, Self::parse_dir(&data)))
    }

    fn resolve(&mut self, path: &str) -> Result<LongAd> {
        let mut cur = self.root_icb;
        for comp in path.split(['/', '\\']).filter(|c| !c.is_empty()) {
            let (_, ents) = self.dir_entries(cur)?;
            match ents
                .iter()
                .find(|e| !e.is_parent && e.characteristics & 0x04 == 0 && e.name.eq_ignore_ascii_case(comp))
            {
                Some(e) => cur = e.icb,
                None => return err(format!("nicht gefunden: {path}")),
            }
        }
        Ok(cur)
    }

    /// Lists a directory (path like "sources" or "efi/boot"; "" = root).
    pub fn list(&mut self, path: &str) -> Result<Vec<(String, bool, u64)>> {
        let icb = self.resolve(path)?;
        let (_, ents) = self.dir_entries(icb)?;
        let mut out = Vec::new();
        for e in ents.iter().filter(|e| !e.is_parent && e.characteristics & 0x04 == 0) {
            let size = if e.is_dir { 0 } else { self.read_node(e.icb.lbn).map(|n| n.info_len).unwrap_or(0) };
            out.push((e.name.clone(), e.is_dir, size));
        }
        Ok(out)
    }

    pub fn exists(&mut self, path: &str) -> bool {
        self.resolve(path).is_ok()
    }

    pub fn read_file(&mut self, path: &str, max: u64) -> Result<Vec<u8>> {
        let icb = self.resolve(path)?;
        let node = self.read_node(icb.lbn)?;
        if node.file_type == 4 {
            return err("ist ein Ordner");
        }
        self.read_node_data(&node, max)
    }

    /// Reads `len` bytes at `offset` of a file (used for the WIM header / XML).
    pub fn read_file_range(&mut self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>> {
        let icb = self.resolve(path)?;
        let node = self.read_node(icb.lbn)?;
        let mut out = Vec::new();
        let mut file_pos = 0u64;
        let end = (offset + len).min(node.info_len);
        if let Some(d) = &node.inline {
            let s = offset.min(d.len() as u64) as usize;
            let e = end.min(d.len() as u64) as usize;
            return Ok(d[s..e].to_vec());
        }
        for e in &node.extents {
            let ext_end = file_pos + e.len as u64;
            if ext_end > offset && file_pos < end {
                let s = offset.max(file_pos);
                let t = end.min(ext_end);
                let mut buf = vec![0u8; (t - s) as usize];
                if e.recorded {
                    let disk = (self.part_start as u64 + e.lbn as u64) * SECTOR + (s - file_pos);
                    self.f.seek(SeekFrom::Start(disk))?;
                    self.f.read_exact(&mut buf)?;
                }
                out.extend_from_slice(&buf);
            }
            file_pos = ext_end;
            if file_pos >= end {
                break;
            }
        }
        Ok(out)
    }

    fn next_unique_id(&mut self, root_entries: &[DirEntry]) -> Result<u64> {
        let mut best = 16u64;
        if let Some(s) = self.lvid_sector {
            let b = self.read_sector(s as u64)?;
            best = best.max(u64le(&b, 40));
        }
        for e in root_entries.iter().filter(|e| !e.is_parent) {
            if let Ok(n) = self.read_node(e.icb.lbn) {
                best = best.max(n.unique_id + 1);
            }
        }
        Ok(best)
    }

    /// Adds (or replaces) files in the ROOT directory. The image must be opened
    /// with `open_rw` and must be a copy – the original is never touched.
    pub fn add_root_files(&mut self, files: &[(&str, &[u8])]) -> Result<()> {
        let root_icb = self.root_icb;
        let (mut root, entries) = self.dir_entries(root_icb)?;
        if root.file_type != 4 {
            return err("UDF: Root ist kein Ordner");
        }
        let parent = match entries.first() {
            Some(p) if p.is_parent => p.clone(),
            _ => return err("UDF: Root-Ordner ohne Parent-Eintrag"),
        };
        // template file entry: first regular file in root
        let template_entry = entries.iter().find(|e| !e.is_parent && !e.is_dir && e.characteristics & 0x04 == 0);
        let template = match template_entry {
            Some(t) => self.read_node(t.icb.lbn)?,
            None => root.clone(),
        };
        // FID CRC convention of this image (padded or unpadded length)
        let fid_crc_padded = entries
            .iter()
            .find(|e| !e.is_parent)
            .map(|e| {
                let l_fi = e.raw[19] as usize;
                let l_iu = u16le(&e.raw, 36) as usize;
                let crclen = u16le(&e.raw, 10) as usize;
                crclen != 38 + l_iu + l_fi - 16
            })
            .unwrap_or(false);

        let mut uid = self.next_unique_id(&entries)?;
        let tag_version = u16le(&root.raw, 2);

        // ---- layout of the appended area
        let mut next_sector = self.image_sectors;
        // keep 16-sector alignment for neatness
        next_sector = next_sector.div_ceil(16) * 16;
        let first_new = next_sector;
        let mut writes: Vec<(u64, Vec<u8>)> = Vec::new();
        let mut new_fids: Vec<Vec<u8>> = Vec::new();

        for (name, data) in files {
            let fe_sector = next_sector;
            let data_sector = fe_sector + 1;
            let data_blocks = (data.len() as u64).div_ceil(SECTOR).max(1);
            next_sector = data_sector + data_blocks;
            let fe_lbn = (fe_sector - self.part_start as u64) as u32;
            let data_lbn = (data_sector - self.part_start as u64) as u32;

            // File Entry (always classic 261 layout)
            let mut fe = vec![0u8; SECTOR as usize];
            fe[..176].copy_from_slice(&template.raw[..176]);
            if template.extended {
                // build classic FE fields from scratch where layouts differ
                fe[..176].fill(0);
                fe[16..36].copy_from_slice(&template.raw[16..36]); // ICB tag
                fe[36..56].copy_from_slice(&template.raw[36..56]); // uid,gid,perm,link,rec*
                fe[72..84].copy_from_slice(&template.raw[80..92]); // access
                fe[84..96].copy_from_slice(&template.raw[92..104]); // modification
                fe[96..108].copy_from_slice(&template.raw[116..128]); // attribute
                fe[128..160].copy_from_slice(&template.raw[168..200]); // impl ident
            }
            put16(&mut fe, 0, 261);
            put16(&mut fe, 2, tag_version);
            put16(&mut fe, 6, u16le(&root.raw, 6)); // serial
            // ICB tag: strategy 4, 1 entry, file type 5 (regular file), short_ad
            put32(&mut fe, 16, 0);
            put16(&mut fe, 20, 4);
            put16(&mut fe, 22, 0);
            put16(&mut fe, 24, 1);
            fe[26] = 0;
            fe[27] = 5;
            fe[28..34].fill(0);
            let tflags = u16le(&fe, 34);
            put16(&mut fe, 34, tflags & !0x0007 & !0x0010 & !0x0020);
            if u32le(&fe, 44) == 0 {
                put32(&mut fe, 44, 0x14A5); // read permissions for everyone
            }
            put16(&mut fe, 48, 1); // link count
            fe[50] = 0;
            fe[51] = 0;
            put32(&mut fe, 52, 0);
            put64(&mut fe, 56, data.len() as u64);
            put64(&mut fe, 64, data_blocks);
            put32(&mut fe, 108, 1); // checkpoint
            fe[112..128].fill(0); // no EA ICB
            put64(&mut fe, 160, uid);
            put32(&mut fe, 168, 0); // L_EA
            put32(&mut fe, 172, 8); // L_AD
            put32(&mut fe, 176, data.len() as u32);
            put32(&mut fe, 180, data_lbn);
            finalize_tag(&mut fe, fe_lbn, 176 + 8 - 16);
            writes.push((fe_sector, fe));

            let mut d = data.to_vec();
            d.resize((data_blocks * SECTOR) as usize, 0);
            writes.push((data_sector, d));

            // FID
            let name_cs0 = encode_cs0(name);
            let l_fi = name_cs0.len();
            let total = (38 + l_fi).div_ceil(4) * 4;
            let mut fid = vec![0u8; total];
            put16(&mut fid, 0, 257);
            put16(&mut fid, 2, tag_version);
            put16(&mut fid, 6, u16le(&parent.raw, 6));
            put16(&mut fid, 16, 1); // file version
            fid[18] = 0;
            fid[19] = l_fi as u8;
            put32(&mut fid, 20, SECTOR as u32);
            put32(&mut fid, 24, fe_lbn);
            put16(&mut fid, 28, parent.icb.part);
            // impUse of long_ad: flags(2) + UDF unique id (4)
            put32(&mut fid, 32, uid as u32);
            put16(&mut fid, 36, 0); // L_IU
            fid[38..38 + l_fi].copy_from_slice(&name_cs0);
            // CRC len is set when the directory is laid out
            new_fids.push(fid);
            uid += 1;
        }

        // ---- new root directory content
        let new_names: Vec<String> = files.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
        let mut fids: Vec<Vec<u8>> = Vec::new();
        for e in &entries {
            if !e.is_parent && new_names.contains(&e.name.to_ascii_lowercase()) {
                continue; // replaced
            }
            fids.push(e.raw.clone());
        }
        let n_old = fids.len();
        fids.extend(new_fids);

        let dir_sector = next_sector;
        let dir_lbn = (dir_sector - self.part_start as u64) as u32;
        let mut dir = Vec::new();
        for (i, mut fid) in fids.into_iter().enumerate() {
            let pos = dir.len();
            let block = dir_lbn + (pos as u64 / SECTOR) as u32;
            if i >= n_old {
                let l_fi = fid[19] as usize;
                let crc_len = if fid_crc_padded { fid.len() - 16 } else { 38 + l_fi - 16 };
                finalize_tag(&mut fid, block, crc_len);
            } else {
                relocate_tag(&mut fid, block);
            }
            dir.extend_from_slice(&fid);
        }
        let dir_len = dir.len() as u64;
        let dir_blocks = dir_len.div_ceil(SECTOR).max(1);
        dir.resize((dir_blocks * SECTOR) as usize, 0);
        writes.push((dir_sector, dir));
        next_sector = dir_sector + dir_blocks;

        // ---- rewrite root (Extended) File Entry in place
        {
            let raw = &mut root.raw;
            let flags = u16le(raw, 34);
            put16(raw, 34, flags & !0x0007); // short_ad
            put64(raw, 56, dir_len);
            let ad_len_off;
            if root.extended {
                put64(raw, 64, dir_len); // object size
                put64(raw, 72, dir_blocks);
                ad_len_off = 212;
            } else {
                put64(raw, 64, dir_blocks);
                ad_len_off = 172;
            }
            let ad_off = root.ad_off;
            raw[ad_off..].fill(0);
            put32(raw, ad_len_off, 8);
            put32(raw, ad_off, dir_len as u32);
            put32(raw, ad_off + 4, dir_lbn);
            let crc_len = ad_off + 8 - 16;
            let _ = root.ea_len;
            finalize_tag(raw, root.lbn, crc_len);
        }
        writes.push((self.part_start as u64 + root.lbn as u64, root.raw.clone()));

        // ---- final AVDP at the new last sector
        let avdp_sector = next_sector;
        let mut avdp = self.avdp.clone();
        let crc_len = u16le(&avdp, 10) as usize;
        finalize_tag(&mut avdp, avdp_sector as u32, crc_len);
        writes.push((avdp_sector, avdp));
        let new_total = avdp_sector + 1;

        // ---- partition length (main + reserve partition descriptors)
        let new_part_len = (new_total - self.part_start as u64) as u32;
        let new_part_len = new_part_len.max(self.part_len);
        for &s in &self.pd_sectors.clone() {
            let mut b = self.read_sector(s as u64)?;
            put32(&mut b, 192, new_part_len);
            let crc_len = u16le(&b, 10) as usize;
            finalize_tag(&mut b, s, crc_len);
            writes.push((s as u64, b));
        }

        // ---- LVID: next unique id, size table, file count
        if let Some(s) = self.lvid_sector {
            let mut b = self.read_sector(s as u64)?;
            let n_parts = u32le(&b, 72) as usize;
            let l_iu = u32le(&b, 76) as usize;
            let crc_len = u16le(&b, 10) as usize;
            if 80 + 8 * n_parts + l_iu <= b.len() && n_parts >= 1 {
                put64(&mut b, 40, uid);
                put32(&mut b, 80 + 4 * n_parts, new_part_len); // size table [0]
                let iu = 80 + 8 * n_parts;
                if l_iu >= 40 {
                    let files_now = u32le(&b, iu + 32);
                    let added = files.len() as u32 - (entries.len() - n_old) as u32;
                    put32(&mut b, iu + 32, files_now.wrapping_add(added));
                }
                finalize_tag(&mut b, s, crc_len);
                writes.push((s as u64, b));
            }
        }

        // ---- ISO9660 volume space size (PVD/SVDs from sector 16)
        for s in 16u64..32 {
            let mut b = self.read_sector(s)?;
            if &b[1..6] != b"CD001" {
                break;
            }
            match b[0] {
                1 | 2 => {
                    b[80..84].copy_from_slice(&(new_total as u32).to_le_bytes());
                    b[84..88].copy_from_slice(&(new_total as u32).to_be_bytes());
                    writes.push((s, b));
                }
                255 => break,
                _ => {}
            }
        }

        let _ = first_new;
        // ---- write everything (appended data first, metadata last)
        writes.sort_by_key(|(s, _)| std::cmp::Reverse(*s >= self.image_sectors));
        for (s, data) in &writes {
            self.f.seek(SeekFrom::Start(s * SECTOR))?;
            self.f.write_all(data)?;
        }
        // zero-fill gap between old end and first appended sector
        self.f.set_len(new_total * SECTOR)?;
        self.f.sync_all()?;
        self.image_sectors = new_total;
        self.part_len = new_part_len;
        Ok(())
    }
}

fn read_sector(f: &mut File, s: u64) -> Result<Vec<u8>> {
    let mut b = vec![0u8; SECTOR as usize];
    f.seek(SeekFrom::Start(s * SECTOR))?;
    let mut got = 0;
    while got < b.len() {
        let n = f.read(&mut b[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(b)
}
