use std::io::{
    Read,
    Result as IOResult,
    Seek,
    SeekFrom,
};
use std::slice;
use std::sync::Arc;

use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::{Vec2, Vec3};
use sourcerenderer_mdl::{
    BodyPart,
    Header,
    Mesh,
    Model,
    PrimitiveRead,
    StringRead,
};
use sourcerenderer_vtx::{
    BodyPartHeader,
    Header as VTXHeader,
    MeshHeader,
    ModelHeader,
    ModelLODHeader,
    StripGroupHeader,
    StripHeader,
    Vertex as VTXVertex,
};
use sourcerenderer_vvd::{
    Header as VVDHeader,
    Vertex,
    VertexFileFixup,
};

use crate::asset::asset_manager::{
    AssetFile,
    DirectlyLoadedAsset,
    MeshRange,
};
use crate::asset::loaders::bsp::Vertex as BspVertex;
use crate::asset::{
    Asset,
    AssetLoadPriority,
    AssetLoader,
    AssetLoaderProgress,
    AssetManager,
    AssetType,
    Mesh as AssetMesh,
    Model as AssetModel,
};
use crate::math::BoundingBox;

const SCALING_FACTOR: f32 = 0.0236f32;

pub struct MDLModelLoader {}

impl<P: Platform> AssetLoader<P> for MDLModelLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path.starts_with("models/") && file.path.ends_with(".mdl")
    }

    #[allow(clippy::never_loop)]
    fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        _priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()> {
        if file.path.contains("autocombine") {
            print!("Model: {} is auto combined", &file.path);
        }

        let mut models = Vec::<Vec<Vec<Mesh>>>::new();
        let file_start = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
        let header = Header::read(&mut file).map_err(|_e| ())?;
        file.seek(SeekFrom::Start(file_start + header.body_part_offset as u64))
            .map_err(|_e| ())?;
        for _ in 0..header.body_part_count {
            let body_part_start = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
            let body_part = BodyPart::read(&mut file).map_err(|_e| ())?;
            let body_part_next = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
            file.seek(SeekFrom::Start(
                body_part_start + body_part.model_index as u64,
            ))
            .map_err(|_e| ())?;
            let mut body_part_models =
                Vec::<Vec<Mesh>>::with_capacity(body_part.models_count as usize);
            for _ in 0..body_part.models_count {
                let model_start = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                let model = Model::read(&mut file).map_err(|_e| ())?;
                let model_next = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                file.seek(SeekFrom::Start(model_start + model.mesh_index as u64))
                    .map_err(|_e| ())?;
                let mut model_meshes = Vec::<Mesh>::with_capacity(model.meshes_count as usize);
                for _ in 0..model.meshes_count {
                    let mesh = Mesh::read(&mut file).map_err(|_e| ())?;
                    let mesh_next = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                    model_meshes.push(mesh);
                    file.seek(SeekFrom::Start(mesh_next)).map_err(|_e| ())?;
                }
                body_part_models.push(model_meshes);
                file.seek(SeekFrom::Start(model_next)).map_err(|_e| ())?;
            }
            models.push(body_part_models);
            file.seek(SeekFrom::Start(body_part_next))
                .map_err(|_e| ())?;
        }

        let mut texture_dirs = Vec::<String>::with_capacity(header.texture_dir_count as usize);
        file.seek(SeekFrom::Start(
            file_start + header.texture_dir_offset as u64,
        ))
        .map_err(|_e| ())?;
        for _ in 0..header.texture_dir_count {
            let offset = file.read_i32().map_err(|_e| ())?;
            let texture_dir_next = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;

            file.seek(SeekFrom::Start(offset as u64)).map_err(|_e| ())?;
            let mut dir = file
                .read_null_terminated_string()
                .map_err(|_e| ())?
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_lowercase();
            if !dir.is_empty() && !dir.ends_with('/') {
                dir += "/";
            }
            texture_dirs.push(dir);
            file.seek(SeekFrom::Start(texture_dir_next))
                .map_err(|_e| ())?;
        }

        let mut textures = Vec::<String>::with_capacity(header.texture_count as usize);
        file.seek(SeekFrom::Start(file_start + header.texture_offset as u64))
            .map_err(|_e| ())?;
        for _ in 0..header.texture_count {
            let texture_start = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
            let texture = sourcerenderer_mdl::Texture::read(&mut file).map_err(|_e| ())?;
            let texture_next = file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;

            file.seek(SeekFrom::Start(texture_start + texture.name_offset as u64))
                .map_err(|_e| ())?;
            textures.push(
                file.read_null_terminated_string()
                    .map_err(|_e| ())?
                    .trim_matches('/')
                    .to_lowercase(),
            );
            file.seek(SeekFrom::Start(texture_next)).map_err(|_e| ())?;
        }

        let mut texture_paths = Vec::<String>::with_capacity(textures.len());
        for texture in &textures {
            if !texture_dirs.is_empty() {
                let mut path = "materials/".to_string();
                path += texture_dirs.first().unwrap();
                path += texture;
                path += ".vmt";
                texture_paths.push(path)
            } else {
                texture_paths.push(texture.clone());
            }

            if texture_dirs.len() > 1 {
                'dirs: for texture_dir in &texture_dirs {
                    let mut path = "materials/".to_string();
                    path += texture_dir;
                    path += texture;
                    path += ".vmt";
                    let path_exists = manager.file_exists(&path);
                    if path_exists {
                        *(texture_paths.last_mut().unwrap()) = path;
                        break 'dirs;
                    }
                }
            }
            manager.request_asset(
                texture_paths.last().unwrap(),
                AssetType::Material,
                AssetLoadPriority::Low,
            );
        }

        let vvd_path = file.path.replace(".mdl", ".vvd");
        let mut vvd_file = manager.load_file(&vvd_path).unwrap();
        let vvd_vertices: Box<[Vertex]> = load_geometry(&mut vvd_file).map_err(|_e| ())?;

        let mut vertices = Vec::<BspVertex>::with_capacity(vvd_vertices.len());

        let vtx_path = file.path.replace(".mdl", ".dx90.vtx");
        let mut vtx_file = manager.load_file(&vtx_path).unwrap();

        let mut ranges = Vec::<MeshRange>::new();
        let mut materials = Vec::<String>::new();
        let mut indices = Vec::<u32>::new();
        let mut strip_group_indices = Vec::<u32>::new();
        let vtx_start = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
        let vtx_header = VTXHeader::read(&mut vtx_file).map_err(|_e| ())?;
        vtx_file
            .seek(SeekFrom::Start(
                vtx_start + vtx_header.body_parts_offset as u64,
            ))
            .map_err(|_e| ())?;
        for body_part_index in 0..vtx_header.body_parts_count {
            let body_part_start = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
            let body_part = BodyPartHeader::read(&mut vtx_file).map_err(|_e| ())?;
            let body_part_next = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
            vtx_file
                .seek(SeekFrom::Start(
                    body_part_start + body_part.model_offset as u64,
                ))
                .map_err(|_e| ())?;
            for model_index in 0..body_part.models_count {
                let model_start = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                let model = ModelHeader::read(&mut vtx_file).map_err(|_e| ())?;
                let model_next = vtx_file.seek(SeekFrom::Start(0)).map_err(|_e| ())?;
                vtx_file
                    .seek(SeekFrom::Start(model_start + model.lod_offset as u64))
                    .map_err(|_e| ())?;
                for _model_lod_index in 0..model.lods_count {
                    let lod_start = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                    let model_lod = ModelLODHeader::read(&mut vtx_file).map_err(|_e| ())?;
                    let lod_next = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                    vtx_file
                        .seek(SeekFrom::Start(lod_start + model_lod.mesh_offset as u64))
                        .map_err(|_e| ())?;
                    for mesh_index in 0..model_lod.meshes_count {
                        let mdl_mesh = &models[body_part_index as usize][model_index as usize]
                            [mesh_index as usize];
                        let indices_start = indices.len();

                        let mesh_start = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                        let mesh = MeshHeader::read(&mut vtx_file).map_err(|_e| ())?;
                        let mesh_next = vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                        vtx_file
                            .seek(SeekFrom::Start(
                                mesh_start + mesh.strip_group_header_offset as u64,
                            ))
                            .map_err(|_e| ())?;
                        for _ in 0..mesh.strip_groups_count {
                            let strip_group_start =
                                vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                            let strip_group =
                                StripGroupHeader::read(&mut vtx_file).map_err(|_e| ())?;
                            let strip_group_next =
                                vtx_file.seek(SeekFrom::Current(0)).map_err(|_e| ())?;
                            vtx_file
                                .seek(SeekFrom::Start(
                                    strip_group_start + strip_group.indices_offset as u64,
                                ))
                                .map_err(|_e| ())?;
                            strip_group_indices.clear();
                            for _ in (0..strip_group.indices_count).rev() {
                                strip_group_indices
                                    .push(vtx_file.read_u16().map_err(|_e| ())? as u32);
                            }

                            let base_index = vertices.len();
                            vtx_file
                                .seek(SeekFrom::Start(
                                    strip_group_start + strip_group.strips_offset as u64,
                                ))
                                .map_err(|_e| ())?;
                            for _ in 0..strip_group.strips_count {
                                let strip = StripHeader::read(&mut vtx_file).map_err(|_e| ())?;
                                for i in 0..strip.indices_count {
                                    indices.push(
                                        base_index as u32
                                            + strip_group_indices
                                                [(strip.index_offset + i) as usize],
                                    );
                                }
                            }

                            vtx_file
                                .seek(SeekFrom::Start(
                                    strip_group_start + strip_group.vert_offset as u64,
                                ))
                                .map_err(|_e| ())?;
                            for _ in 0..strip_group.verts_count {
                                let vtx_vertex = VTXVertex::read(&mut vtx_file).map_err(|_e| ())?;
                                let vert_index =
                                    mdl_mesh.vertex_offset + vtx_vertex.orig_mesh_vert_id as i32;
                                if vert_index < 0 || vert_index as usize >= vvd_vertices.len() {
                                    return Err(());
                                }
                                let vertex = vvd_vertices.as_ref()[vert_index as usize].clone();
                                let bsp_vertex = BspVertex {
                                    position: fixup_position(&vertex.vec_position),
                                    normal: fixup_normal(&vertex.vec_normal),
                                    uv: vertex.vec_tex_coord,
                                    lightmap_uv: Vec2::new(0f32, 0f32),
                                    alpha: 0.0,
                                    ..Default::default()
                                };
                                vertices.push(bsp_vertex);
                            }

                            vtx_file
                                .seek(SeekFrom::Start(strip_group_next))
                                .map_err(|_e| ())?;
                        }

                        materials.push(texture_paths[mdl_mesh.material as usize].clone());
                        ranges.push(MeshRange {
                            start: indices_start as u32,
                            count: (indices.len() - indices_start) as u32,
                        });
                        vtx_file.seek(SeekFrom::Start(mesh_next)).map_err(|_e| ())?;
                    }
                    vtx_file.seek(SeekFrom::Start(lod_next)).map_err(|_e| ())?;
                    break;
                }
                vtx_file
                    .seek(SeekFrom::Start(model_next))
                    .map_err(|_e| ())?;
            }
            vtx_file
                .seek(SeekFrom::Start(body_part_next))
                .map_err(|_e| ())?;
        }

        let indices_box = indices.clone().into_boxed_slice();
        let indices_count = indices.len();
        let ptr = Box::into_raw(indices_box);
        let data_ptr = unsafe {
            slice::from_raw_parts_mut(ptr as *mut u8, indices_count * std::mem::size_of::<u32>())
                as *mut [u8]
        };
        let indices_data = unsafe { Box::from_raw(data_ptr) };
        let vertices_box = vertices.clone().into_boxed_slice();
        let vertices_count = vertices.len();
        let ptr = Box::into_raw(vertices_box);
        let data_ptr = unsafe {
            slice::from_raw_parts_mut(
                ptr as *mut u8,
                vertices_count * std::mem::size_of::<BspVertex>(),
            ) as *mut [u8]
        };
        let vertices_data = unsafe { Box::from_raw(data_ptr) };

        let hull_min = fixup_position(&header.hull_min);
        let hull_max = fixup_position(&header.hull_max);
        let min = Vec3::new(
            hull_min.x.min(hull_max.x),
            hull_min.y.min(hull_max.y),
            hull_min.z.min(hull_max.z),
        );
        let max = Vec3::new(
            hull_min.x.max(hull_max.x),
            hull_min.y.max(hull_max.y),
            hull_min.z.max(hull_max.z),
        );

        manager.add_asset(
            &vtx_path,
            Asset::Mesh(AssetMesh {
                indices: Some(indices_data),
                vertices: vertices_data,
                parts: ranges.into_boxed_slice(),
                bounding_box: Some(BoundingBox::new(min, max)),
                vertex_count: vertices_count as u32,
            }),
            AssetLoadPriority::Normal,
        );

        manager.add_asset_with_progress(
            &file.path,
            Asset::Model(AssetModel {
                mesh_path: vtx_path,
                material_paths: materials,
            }),
            Some(progress),
            AssetLoadPriority::Normal,
        );

        Ok(DirectlyLoadedAsset::None)
    }
}

impl MDLModelLoader {
    pub fn new() -> Self {
        Self {}
    }
}

#[allow(clippy::never_loop)]
fn load_geometry<R: Read + Seek>(file: &mut R) -> IOResult<Box<[Vertex]>> {
    let vvd_start = file.seek(SeekFrom::Current(0))?;
    let vvd_header = VVDHeader::read(file)?;

    let mut original_vertices = Vec::<Vertex>::new();
    file.seek(SeekFrom::Start(
        vvd_start + vvd_header.vertex_data_start as u64,
    ))?;
    for lod_index in 0..vvd_header.lods_count {
        for _ in 0..vvd_header.lod_vertexes_count[lod_index as usize] {
            original_vertices.push(Vertex::read(file)?);
        }
        break; // TODO: support LODs
    }
    let mut vertices = original_vertices.clone();

    file.seek(SeekFrom::Start(
        vvd_start + vvd_header.fixup_table_start as u64,
    ))?;
    let mut vertex_index = 0;
    for _ in 0..vvd_header.fixups_count {
        let fixup = VertexFileFixup::read(file)?;
        if fixup.lod < 0 {
            // TODO: support LODs
            continue;
        }
        vertices[vertex_index as usize..(vertex_index + fixup.vertices_count) as usize]
            .clone_from_slice(
                &original_vertices[fixup.source_vertex_id as usize
                    ..(fixup.source_vertex_id + fixup.vertices_count) as usize],
            );
        vertex_index += fixup.vertices_count;
    }

    Ok(vertices.into_boxed_slice())
}

fn fixup_position(position: &Vec3) -> Vec3 {
    Vec3::new(position.x, position.z, position.y) * SCALING_FACTOR
}

fn fixup_normal(normal: &Vec3) -> Vec3 {
    Vec3::new(normal.x, normal.z, normal.y)
}
