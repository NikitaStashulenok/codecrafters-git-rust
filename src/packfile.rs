use std::io::{Cursor, Read};

use anyhow::Result;
use bytes::{Buf, Bytes};
use flate2::bufread::ZlibDecoder;
use sha1::{Digest, Sha1};

use crate::my_git::object::{GitObject, ObjectType};

#[derive(Debug)]
pub enum PackObject {
    Commit,
    Tree,
    Blob,
    Tag,
    OfsDelta,
    RefDelta,
}

impl PackObject {
    pub fn from(b: u8) -> Result<Self> {
        match b {
            1 => Ok(Self::Commit),
            2 => Ok(Self::Tree),
            3 => Ok(Self::Blob),
            4 => Ok(Self::Tag),
            6 => Ok(Self::OfsDelta),
            7 => Ok(Self::RefDelta),
            _ => anyhow::bail!("not a pack object"),
        }
    }
}

#[derive(Debug)]
pub enum DeltaInstruction {
    Copy { size: u64, offset: u64 },
    Add { data: Bytes },
}

#[derive(Debug, Default)]
pub struct PackHeader {
    signature: String,
    version: u8,
    obj_count: u32,
}

#[derive(Debug)]
pub struct PackFile {
    header: PackHeader,
    content: Cursor<Bytes>,
}

impl PackFile {
    pub fn new(mut data: Bytes) -> Result<Self> {
        let content = data.split_to(data.len() - 20);
        let checksum = hex::encode(data);

        let mut encoder = Sha1::new();
        encoder.update(&content);
        let check = hex::encode(encoder.finalize());
        anyhow::ensure!(check == checksum, "failed to verify checksum");

        Ok(Self {
            header: PackHeader::default(),
            content: Cursor::new(content),
        })
    }

    pub fn parse(&mut self) -> Result<()> {
        let sig = self.content.get_u32().to_be_bytes();
        let version = self.content.get_u32();
        let obj_count = self.content.get_u32();

        anyhow::ensure!(&sig == b"PACK", "failed to verify pack signature");
        anyhow::ensure!(version == 2, "failed to verify pack version");

        self.header.signature = String::from("PACK");
        self.header.version = 2;
        self.header.obj_count = obj_count;

        let mut parsed_obj = 0;
        while self.content.has_remaining() {
            let (obj_type, size) = self.get_obj_type_and_size()?;
            match obj_type {
                PackObject::RefDelta => self.parse_ref_delta(size)?,
                PackObject::OfsDelta => self.parse_ofs_delta()?,
                PackObject::Commit => self.parse_git_object(ObjectType::Commit, size)?,
                PackObject::Tree => self.parse_git_object(ObjectType::Tree, size)?,
                PackObject::Blob => self.parse_git_object(ObjectType::Blob, size)?,
                PackObject::Tag => self.parse_git_object(ObjectType::Tag, size)?,
            };
            parsed_obj += 1;
        }

        anyhow::ensure!(parsed_obj == obj_count);
        Ok(())
    }

    fn get_obj_type_and_size(&mut self) -> Result<(PackObject, u64)> {
        let lead = self.content.get_u8();
        let obj_type = (lead & 0b0111_0000) >> 4;
        let obj_type = PackObject::from(obj_type)?;
        let mut size = (lead & 0b0000_1111) as u64;

        if (lead >> 7) & 1 == 0 {
            return Ok((obj_type, size));
        }

        let mut step = 0;
        loop {
            let byte = self.content.get_u8();
            size |= ((byte & 0b0111_1111) as u64) << (7 * step + 4);

            if (byte >> 7) & 1 == 0 {
                return Ok((obj_type, size));
            }

            step += 1;
        }
    }

    fn parse_ref_delta(&mut self, size: u64) -> Result<()> {
        let mut file_ref = [0u8; 20];
        self.content.read_exact(&mut file_ref)?;

        let file_ref = hex::encode(file_ref);
        let base_obj = GitObject::from_ref(&file_ref)?;
        let base_content = base_obj.raw_content();

        let mut body = Vec::new();
        let mut decoder = ZlibDecoder::new(self.content.clone());
        decoder.read_to_end(&mut body)?;

        anyhow::ensure!(decoder.total_out() == size);
        self.content.advance(decoder.total_in() as usize);

        let mut delta = Bytes::from(body);

        let source_size = get_ref_delta_size(&mut delta);
        anyhow::ensure!(source_size == base_content.len() as u64);

        let target_size = get_ref_delta_size(&mut delta);
        let instructions = get_delta_instructions(&mut delta)?;

        let mut content = Vec::new();
        for instr in instructions {
            match instr {
                DeltaInstruction::Copy { size, offset } => {
                    content.extend(&base_content[offset as usize..(offset + size) as usize])
                }
                DeltaInstruction::Add { data } => content.extend(data),
            }
        }

        anyhow::ensure!(target_size == content.len() as u64);

        let obj = GitObject::new(*base_obj.object_type(), content)?;
        obj.write()
    }

    fn parse_ofs_delta(&mut self) -> Result<()> {
        let mut offset = 0;
        loop {
            let byte = self.content.get_u8();
            offset = (offset << 7) | ((byte & 0b0111_1111) as u64);
            if (byte >> 7) & 1 == 0 {
                break;
            }
        }

        let pos = self.content.position();
        self.content.set_position(pos - offset);

        let mut content = Vec::new();
        let mut decoder = ZlibDecoder::new(self.content.clone());
        decoder.read_to_end(&mut content)?;

        // Nothing to do with ofs_delta content now!

        self.content.set_position(pos);

        Ok(())
    }

    fn parse_git_object(&mut self, obj_type: ObjectType, size: u64) -> Result<()> {
        let mut content = Vec::with_capacity(size as usize);
        let mut decoder = ZlibDecoder::new(self.content.clone());
        decoder.read_to_end(&mut content)?;

        anyhow::ensure!(decoder.total_out() == size);
        self.content.advance(decoder.total_in() as usize);

        let obj = GitObject::new(obj_type, content)?;
        obj.write()
    }
}

fn get_ref_delta_size(delta: &mut Bytes) -> u64 {
    let mut size = 0;
    let mut i = 0;
    loop {
        let byte = delta.get_u8();
        size |= ((byte & 0b0111_1111) as u64) << (7 * i);
        if (byte >> 7) & 1 == 0 {
            break;
        }
        i += 1;
    }
    size
}

fn get_delta_instructions(delta: &mut Bytes) -> Result<Vec<DeltaInstruction>> {
    let mut instrutions = Vec::new();

    while delta.has_remaining() {
        let lead = delta.get_u8();
        if (lead >> 7) & 1 == 0 {
            let data = delta.split_to(lead as usize);
            instrutions.push(DeltaInstruction::Add { data });
        } else {
            let mut offset = 0;
            let mut size = 0;

            for i in 0..=3u8 {
                if (lead >> i) & 1 == 1 {
                    let byte = delta.get_u8();
                    offset |= (byte as u64) << (8 * i as u64);
                }
            }

            for i in 4..=6u8 {
                if (lead >> i) & 1 == 1 {
                    let byte = delta.get_u8();
                    size |= (byte as u64) << (8 * (i as u64 - 4));
                }
            }

            instrutions.push(DeltaInstruction::Copy { size, offset });
        }
    }

    Ok(instrutions)
}
