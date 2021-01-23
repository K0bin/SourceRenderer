use std::io::{Read, Seek, Result as IOResult, Error as IOError, SeekFrom, ErrorKind};
use crate::header::Header;
use crate::header2::Header2;
use crate::{Bone, BoneController, HitboxSet, AnimDesc, SequenceDesc, Texture, StringRead, PrimitiveRead, BodyPart, Model, Mesh};

pub struct ModelFile<R: Read + Seek> {
  header: Header,
  secondary_header: Header2,
  reader: R,
  start_offset: u64
}

impl<R: Read + Seek> ModelFile<R> {
  pub fn read(mut reader: R) -> IOResult<Self> {
    let start = reader.seek(SeekFrom::Current(0)).unwrap();
    let header = Header::read(&mut reader)?;

    if header.id != 0x54534449 {
      return Err(IOError::new(ErrorKind::Other, "Not a MDL file."));
    }

    let mut textures = Vec::<Texture>::with_capacity(header.texture_count as usize);
    reader.seek(SeekFrom::Start(start + header.texture_offset as u64));
    for _ in 0..header.texture_count {
      textures.push(Texture::read(&mut reader)?);
    }

    reader.seek(SeekFrom::Start(start + header.studio_hdr2_index as u64));
    let header2 = Header2::read(&mut reader)?;

    Ok(Self {
      header,
      secondary_header: header2,
      reader,
      start_offset: start
    })
  }

  pub fn bones(&mut self) -> IOResult<Vec<Bone>> {
    let mut bones = Vec::<Bone>::with_capacity(self.header.bone_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.bone_offset as u64));
    for _ in 0..self.header.bone_count {
      bones.push(Bone::read(&mut self.reader)?);
    }
    Ok(bones)
  }

  pub fn bone_controllers(&mut self) -> IOResult<Vec<BoneController>> {
    let mut bone_controllers = Vec::<BoneController>::with_capacity(self.header.bone_controller_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.bone_controller_offset as u64));
    for _ in 0..self.header.bone_controller_count {
      bone_controllers.push(BoneController::read(&mut self.reader)?);
    }
    Ok(bone_controllers)
  }

  pub fn hitbox_sets(&mut self) -> IOResult<Vec<HitboxSet>> {
    let mut hitboxes = Vec::<HitboxSet>::with_capacity(self.header.hitbox_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.hitbox_offset as u64));
    for _ in 0..self.header.hitbox_count {
      hitboxes.push(HitboxSet::read(&mut self.reader)?);
    }
    Ok(hitboxes)
  }

  pub fn animation_descs(&mut self) -> IOResult<Vec<AnimDesc>> {
    let mut anims = Vec::<AnimDesc>::with_capacity(self.header.local_anim_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.local_anim_offset as u64));
    for _ in 0..self.header.local_anim_count {
      anims.push(AnimDesc::read(&mut self.reader)?);
    }
    Ok(anims)
  }

  pub fn sequence_descs(&mut self) -> IOResult<Vec<SequenceDesc>> {
    let mut seqs = Vec::<SequenceDesc>::with_capacity(self.header.local_seq_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.local_seq_offset as u64));
    for _ in 0..self.header.local_seq_count {
      seqs.push(SequenceDesc::read(&mut self.reader)?);
    }
    Ok(seqs)
  }

  pub fn textures(&mut self) -> IOResult<Vec<(String, Texture)>> {
    let mut textures = Vec::<(String, Texture)>::with_capacity(self.header.texture_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.texture_offset as u64));
    for _ in 0..self.header.texture_count {
      let texture = Texture::read(&mut self.reader)?;
      let offset = self.reader.seek(SeekFrom::Current(0)).unwrap();
      self.reader.seek(SeekFrom::Start(offset + texture.name_offset as u64));
      let name = self.reader.read_null_terminated_string().unwrap();
      textures.push((name, texture));
    }
    Ok(textures)
  }

  pub fn texture_dirs(&mut self) -> IOResult<Vec<String>> {
    let mut dir_offsets = Vec::<i32>::with_capacity(self.header.texture_dir_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.texture_dir_offset as u64));
    for _ in 0..self.header.texture_dir_count {
      dir_offsets.push(self.reader.read_i32()?);
    }
    let mut texture_dirs = Vec::<String>::with_capacity(self.header.texture_dir_count as usize);
    for offset in dir_offsets {
      self.reader.seek(SeekFrom::Start(self.start_offset + offset as u64));
      texture_dirs.push(self.reader.read_fixed_length_null_terminated_string(255).unwrap());
    }
    Ok(texture_dirs)
  }

  pub fn body_parts(&mut self) -> IOResult<Vec<(String, BodyPart)>> {
    let mut body_parts = Vec::<(String, BodyPart)>::with_capacity(self.header.body_part_count as usize);
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.body_part_offset as u64));
    for _ in 0..self.header.body_part_count {
      let mut body_part = BodyPart::read(&mut self.reader)?;
      let start = self.reader.seek(SeekFrom::Current(0)).unwrap();
      body_part.model_index += start;
      self.reader.seek(SeekFrom::Start(start + body_part.name_index as u64));
      let name = self.reader.read_null_terminated_string().unwrap();
      body_parts.push((name, body_part));
      self.reader.seek(SeekFrom::Start(start));
    }
    Ok(body_parts)
  }

  pub fn models(&mut self, model_offset: u64, model_count: u32) -> IOResult<Vec<Model>> {
    let mut models = Vec::<Model>::with_capacity(model_count as usize);
    self.reader.seek(SeekFrom::Start(model_offset));
    for _ in 0..model_count {
      let mut model = Model::read(&mut self.reader)?;
      let start = self.reader.seek(SeekFrom::Current(0)).unwrap();
      model.mesh_index += start;
      models.push(model);
    }
    Ok(models)
  }

  pub fn meshes(&mut self, mesh_offset: u64, mesh_count: u32) -> IOResult<Vec<Mesh>> {
    let mut meshes = Vec::<Mesh>::with_capacity(mesh_count as usize);
    self.reader.seek(SeekFrom::Start(mesh_offset));
    for _ in 0..mesh_count {
      let mesh = Mesh::read(&mut self.reader)?;
      meshes.push(mesh);
    }
    Ok(meshes)
  }

  pub fn header(&self) -> &Header {
    &self.header
  }

  pub fn header2(&mut self) -> IOResult<Header2> {
    self.reader.seek(SeekFrom::Start(self.start_offset + self.header.studio_hdr2_index as u64));
    Header2::read(&mut self.reader)
  }
}

