use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::uni_project::{manix_transform, MANIX_PATH, make_volume_material, MESHES};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::change_detection::{NonSendMut, Res, ResMut};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Commands;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::Query;
use bevy_math::Affine3A;
use bytemuck::box_bytes_of;
use sourcerenderer_core::gpu::{Format, SampleCount, TextureDimension, TextureInfo, TextureUsage};
use sourcerenderer_engine::dear_imgui_rs::{ChildWindow, ColorEditFlags, Condition, ListBox, TextureId, TextureRef};
use sourcerenderer_engine::renderer::VolumeMeshInstance;
use sourcerenderer_engine::renderer::VolumeRendererOptions;
use sourcerenderer_engine::transform::InterpolatedTransform;
use sourcerenderer_engine::{dear_imgui_rs, DearImgui, VolumeDrawableTransparencyMode};
use sourcerenderer_engine::asset::{AssetData, AssetHandle, AssetLoadPriority, AssetManager, AssetManagerECSResource, AssetType, TextureData, TextureHandle};
use smallvec::smallvec;

pub(super) struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UIState::default());
        app.add_systems(Update, (volume_meshes_ui_system, pick_hdri_ui_system, import_materials_ui_system, materials_ui_system));
    }
}

fn pick_hdri_ui_system(
    imgui: NonSendMut<DearImgui>,
    mut options: ResMut<VolumeRendererOptions>,
) {
    let ui = imgui.ui();
    let hdris = [
        ("None", None),
        ("Blaubeuren Night", Some("assets/BlaubeurenNight1k.hdr")),
        ("Environment", Some("assets/environment.hdr")),
        (
            "Little Paris Eiffel Tower",
            Some("assets/little_paris_eiffel_tower_4k.hdr"),
        ),
    ];

    let mut current_idx = hdris
        .iter()
        .position(|(_, path)| *path == options.background_hdri.as_ref().map(|s| s.as_str()))
        .unwrap_or(0);

    ui.window("Environment##environmentwindow")
        .position([520.0, 0.0], Condition::FirstUseEver)
        .size([300.0, 100.0], Condition::FirstUseEver)
        .build(|| {
            ui.text("Background HDRI:");
            ui.set_next_item_width(ui.content_region_avail_width());
            if ui.combo(
                "##background_hdri",
                &mut current_idx,
                &hdris,
                |&(name, _)| name.into(),
            ) {
                options.background_hdri = hdris[current_idx].1.map(|s| s.to_string());
            }
        });
}

struct Colors<const STEPS: usize> {
    positions: [f32; STEPS],
    values: [[f32; 4]; STEPS],
}

impl<const STEPS: usize> Default for Colors<STEPS> {
    fn default() -> Self {
        let mut positions = [0.0f32; STEPS];
        let values = [[0.005f32, 0.005f32, 0.005f32, 1.0f32]; STEPS];
        let fraction = 1.0f32 / (STEPS as f32);
        for i in 0..STEPS {
            positions[i] = (i as f32) * fraction;
        }
        Self {
            positions,
            values,
        }
    }
}

const TEXTURE_GRADIENT_STEPS: usize = 3;

#[derive(Default)]
struct MaterialUIState<const STEPS: usize> {
    albedo: Colors<STEPS>,
    roughness: Colors<STEPS>,
    metalness: Colors<STEPS>,
}

#[derive(Default, Resource)]
struct UIState {
    selected: Option<Entity>,
    selected_material: Option<String>,
    materials: HashMap<String, MaterialUIState<TEXTURE_GRADIENT_STEPS>>,
    next_material_id: u64,
}

fn volume_meshes_ui_system(
    imgui: NonSendMut<DearImgui>,
    mut instances: Query<(Entity, &mut VolumeMeshInstance)>,
    mut state: ResMut<UIState>,
    mut commands: Commands,
    asset_manager: Res<AssetManagerECSResource>
) {
    let ui = imgui.ui();
    let window_size = [500.0f32, 400.0f32];

    let material_paths: Vec<String> = {
        let material_keys = state.materials.keys();
        material_keys.into_iter().cloned().collect()
    };

    ui.window("Meshes##meshwindow")
        .position([0.0, 0.0], Condition::FirstUseEver)
        .size(window_size, Condition::FirstUseEver)
        .build(|| {
            let size = ui.content_region_avail();
            ChildWindow::new("##meshchildwindow")
                .size([128.0f32.min(size[0]), size[1]])
                .build(ui, || {
                    ListBox::new("##meshlistbox")
                        .size([
                            ui.content_region_avail()[0],
                            (ui.content_region_avail()[1] - 64.0f32).max(0.0f32),
                        ])
                        .build(ui, || {
                            for (entity, mesh) in &instances {
                                let text = format!(
                                    "Mesh {:?}##meshlistentry{:?}",
                                    mesh.threshold_min, entity
                                );
                                if ui
                                    .selectable_config(&text)
                                    .selected(state.selected == Some(entity))
                                    .build()
                                {
                                    state.selected = Some(entity);
                                }
                            }
                        });
                    if ui.button("Add mesh##addmeshbutton") {
                        let mut new_entity = commands.spawn_empty();
                        let id = new_entity.id();

                        let material_path = format!("VolumeMaterial_{}", id);
                        asset_manager.add_asset_data(&material_path, AssetData::Material(make_volume_material(&material_path)), AssetLoadPriority::High);

                        new_entity.insert((
                                VolumeMeshInstance {
                                    volume_texture_path: MANIX_PATH.to_string(),
                                    volume_texture_lod: 3,
                                    material_path,
                                    threshold_min: 0.95f32,
                                    transparent: VolumeDrawableTransparencyMode::Opaque,
                                    render_as_cubes: false,
                                    ray_march_normals: false,
                                },
                                InterpolatedTransform(Affine3A::from_mat4(manix_transform())),
                            ));
                        state.selected = Some(id);
                    }
                });

            ui.same_line();
            ChildWindow::new("##meshproperties").build(ui, || {
                if let Some(entity) = state.selected {
                    if let Ok((_, mut mesh)) = instances.get_mut(entity) {
                        let mut idx = MESHES.iter().enumerate().find(|(_, path)| **path == mesh.volume_texture_path).map(|(idx, _)| idx).unwrap_or(0);
                        ui.text("Mesh:");
                        if ui.combo(
                            "##mesh",
                            &mut idx,
                            &MESHES,
                            |path| (*path).into(),
                        ) {
                            mesh.volume_texture_path = MESHES[idx].to_string();
                        }

                        ui.text("Min Threshold:");
                        ui.set_next_item_width(ui.content_region_avail_width());
                        ui.slider(
                            format!("##minthreshold{:?}", entity),
                            0.01f32,
                            1.0f32,
                            &mut mesh.threshold_min,
                        );

                        ui.text("Transparency:");
                        if ui.radio_button(
                            "Opaque##transparency0",
                            mesh.transparent == VolumeDrawableTransparencyMode::Opaque,
                        ) {
                            mesh.transparent = VolumeDrawableTransparencyMode::Opaque;
                        }
                        if ui.radio_button(
                            "Transparent##transparency1",
                            mesh.transparent == VolumeDrawableTransparencyMode::Transparent,
                        ) {
                            mesh.transparent = VolumeDrawableTransparencyMode::Transparent;
                        }
                        if ui.radio_button(
                            "Transparent in front of opaque##transparency1",
                            mesh.transparent
                                == VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque,
                        ) {
                            mesh.transparent =
                                VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque;
                        }

                        ui.text("LOD:");
                        ui.set_next_item_width(ui.content_region_avail_width());
                        ui.slider(
                            format!("##lod{:?}", entity),
                            0u32,
                            4u32,
                            &mut mesh.volume_texture_lod,
                        );

                        ui.text("Render as cubes:");
                        ui.same_line();
                        ui.checkbox(
                            format!("##renderascubes{:?}", entity),
                            &mut mesh.render_as_cubes,
                        );

                        ui.text("Raymarch normals:");
                        ui.same_line();
                        ui.checkbox(
                            format!("##raymarchnormals{:?}", entity),
                            &mut mesh.ray_march_normals,
                        );

                        let mut current_idx = material_paths.iter().enumerate().find_map(|(idx, path)| if path == &mesh.material_path {
                            Some(idx)
                        } else {
                            None
                        }).unwrap_or(0);

                        if ui.combo(
                            "##mesh_material",
                            &mut current_idx,
                            &material_paths,
                            |path| path.into(),
                        ) {
                            mesh.material_path = material_paths[current_idx].clone();
                        }

                        if ui.button("Delete mesh##deletemeshbutton") {
                            commands.entity(entity).despawn();
                            state.selected = None;
                        }
                    }
                }
            });
        });
}


fn import_materials_ui_system(
    instances: Query<(Entity, &VolumeMeshInstance)>,
    mut state: ResMut<UIState>,
    asset_manager: Res<AssetManagerECSResource>
) {
    let mut keys_to_add = HashSet::<String>::with_capacity(state.materials.len());
    for (_, instance) in &instances {
        if instance.material_path == "" {
            continue;
        }
        keys_to_add.insert(instance.material_path.clone());
    }
    for key in state.materials.keys() {
        keys_to_add.remove(key);
    }
    for material_path in keys_to_add {
        asset_manager.add_asset_data(&material_path, AssetData::Material(make_volume_material(&material_path)), AssetLoadPriority::High);

        let albedo_path = format!("{}_albedo", &material_path);
        let roughness_path = format!("{}_roughness", &material_path);
        let metalness_path = format!("{}_metalnness", &material_path);

        let _ = asset_manager.get_or_reserve_handle(&albedo_path, AssetType::Texture);
        let _ = asset_manager.get_or_reserve_handle(&roughness_path, AssetType::Texture);
        let _ = asset_manager.get_or_reserve_handle(&metalness_path, AssetType::Texture);

        let material_ui = MaterialUIState::default();
        const TEXTURE_WIDTH: u32 = 128;
        update_texture(&albedo_path, TEXTURE_WIDTH, false, &material_ui.albedo, &asset_manager);
        update_texture(&roughness_path, TEXTURE_WIDTH, true, &material_ui.roughness, &asset_manager);
        update_texture(&metalness_path, TEXTURE_WIDTH, true, &material_ui.metalness, &asset_manager);
        state.materials.insert(material_path, material_ui);
    }
}


fn materials_ui_system(
    imgui: NonSendMut<DearImgui>,
    mut instances: Query<(Entity, &mut VolumeMeshInstance)>,
    mut state: ResMut<UIState>,
    mut commands: Commands,
    asset_manager: Res<AssetManagerECSResource>
) {
    let ui = imgui.ui();
    let window_size = [500.0f32, 400.0f32];

    ui.window("Materials##materialwindow")
        .position([0.0, 0.0], Condition::FirstUseEver)
        .size(window_size, Condition::FirstUseEver)
        .build(|| {
            let size = ui.content_region_avail();
            ChildWindow::new("##materialchildwindow")
                .size([128.0f32.min(size[0]), size[1]])
                .build(ui, || {
                    ListBox::new("##materiallistbox")
                        .size([
                            ui.content_region_avail()[0],
                                  (ui.content_region_avail()[1] - 64.0f32).max(0.0f32),
                        ])
                        .build(ui, || {
                            let mut new_selected: Option<String> = None;
                            for path in state.materials.keys() {
                                let text = format!(
                                    "{:?}##materials{:?}",
                                    path, path
                                );
                                if ui
                                    .selectable_config(&text)
                                    .selected(state.selected_material.as_ref() == Some(path))
                                    .build()
                                {
                                    new_selected = Some(path.clone());
                                }
                            }
                            if new_selected.is_some() {
                                state.selected_material = new_selected;
                            }
                        });
                    if ui.button("Add Material##addmaterialbutton") {
                        let new_entity = commands.spawn_empty();
                        let id = new_entity.id();

                        let material_path = format!("VolumeMaterial_{}", state.next_material_id);
                        state.next_material_id += 1;
                        asset_manager.add_asset_data(&material_path, AssetData::Material(make_volume_material(&material_path)), AssetLoadPriority::High);

                        state.materials.insert(material_path, MaterialUIState::default());

                        state.selected = Some(id);
                    }
                });

            ui.same_line();
            ChildWindow::new("##materialproperties").build(ui, || {
                if let Some(material_path) = state.selected_material.as_ref().cloned() {
                    let mut delete = false;
                    if let Some(material) = state.materials.get_mut(&material_path) {
                        let albedo_path = format!("{}_albedo", material_path);
                        let roughness_path = format!("{}_roughness", material_path);
                        let metalness_path = format!("{}_metalnness", material_path);

                        let albedo_handle: TextureHandle = asset_manager.get_or_reserve_handle(&albedo_path, AssetType::Texture).into();
                        let roughness_handle: TextureHandle = asset_manager.get_or_reserve_handle(&roughness_path, AssetType::Texture).into();
                        let metalness_handle: TextureHandle = asset_manager.get_or_reserve_handle(&metalness_path, AssetType::Texture).into();

                        const TEXTURE_WIDTH: u32 = 128;
                        ui.text("Albedo:");
                        if color_gradient(ui,false, &mut material.albedo.positions, &mut material.albedo.values,
                                          &format!("albedo{:?}", material_path), albedo_handle) {
                            update_texture(&albedo_path, TEXTURE_WIDTH, false, &material.albedo, &asset_manager);
                        }
                        ui.text("Roughness:");
                        if color_gradient(ui, true, &mut material.roughness.positions, &mut material.roughness.values, &format!("roughness{:?}", material_path), roughness_handle) {
                            update_texture(&roughness_path, TEXTURE_WIDTH, true, &material.roughness, &asset_manager);
                        }
                        ui.text("Metalness:");
                        if color_gradient(ui, true, &mut material.metalness.positions, &mut material.metalness.values, &format!("metalness{:?}", material_path), metalness_handle) {
                            update_texture(&metalness_path, TEXTURE_WIDTH, true, &material.metalness, &asset_manager);
                        }

                        // TODO: Rate limit updates

                        if ui.button("Delete material##deletematerialbutton") {
                            delete = true;
                            state.selected_material = None;
                        }
                    }
                    if delete {
                        state.materials.remove(&material_path);
                        for (_, mut mesh) in instances.iter_mut() {
                            if &mesh.material_path == &material_path {
                                if state.materials.is_empty() {
                                    mesh.material_path = "".to_string();
                                } else {
                                    mesh.material_path = state.materials.keys().find(|_| true).unwrap().clone();
                                }
                            }
                        }
                    }
                }
            });
        });

}

fn color_gradient<const STEPS: usize>(ui: &dear_imgui_rs::Ui, greyscale: bool, positions: &mut [f32; STEPS], colors: &mut [[f32; 4]; STEPS], imgui_label_internal: &str, texture_handle: TextureHandle) -> bool {
    let space = unsafe { ui.style() }.item_spacing()[0];
    let size = (ui.content_region_avail()[0] - ((STEPS - 1) as f32 * space)) * (1.0f32 / (STEPS as f32));

    let asset_handle: AssetHandle = texture_handle.into();
    let imgui_texture_id = dear_imgui_rs::TextureId::new(asset_handle.index());
    let imgui_texture_ref = dear_imgui_rs::TextureRef::from(imgui_texture_id);
    ui.image(imgui_texture_ref, [ui.content_region_avail()[0], ui.frame_height() * 2.0f32]);

    let mut changed = false;

    for i in 0..STEPS {
        let mut previous_pos = 0.0f32;
        let mut next_pos = 1.0f32;
        if i != 0 {
            ui.same_line();
            previous_pos = positions[i - 1];
        }
        if i != STEPS - 1 {
            next_pos = positions[i + 1];
        }
        ui.set_next_item_width(size);
        changed |= ui.slider_f32(format!("##{}_{}", imgui_label_internal, i), &mut positions[i], previous_pos, next_pos);
    }

    for i in 0..STEPS {
        let color = &mut colors[i];
        if i != 0 {
            ui.same_line();
        }
        if !greyscale {
            if i != 0 {
                ui.set_cursor_pos_x((size + space) * (i as f32));
            }
            ui.set_next_item_width(size);
            changed |= ui.color_edit4_config(format!("##{}_color_{}", imgui_label_internal, i), color)
                .flags(ColorEditFlags::NO_INPUTS | ColorEditFlags::NO_LABEL)
                .build();
        } else {
            ui.set_next_item_width(size - ui.frame_height() - space);
            // Roughness 0 is broken, set it to 0.005
            changed |= ui.slider_f32(format!("##{}_color_{}", imgui_label_internal, i), &mut color[0], 0.005f32, 1.0f32);
            color[1] = color[0];
            color[2] = color[0];
            color[3] = 1.0f32;
            ui.same_line();
            let _ = ui.color_edit4_config(format!("##{}_color_preview_{}", imgui_label_internal, i), color)
                .flags(ColorEditFlags::NO_INPUTS | ColorEditFlags::NO_LABEL | ColorEditFlags::ALPHA_OPAQUE | ColorEditFlags::NO_PICKER | ColorEditFlags::NO_ALPHA)
                .build();
        }
    }

    changed
}

fn update_texture(path: &str, width: u32, greyscale: bool, colors: &Colors<TEXTURE_GRADIENT_STEPS>, asset_manager: &Arc<AssetManager>) {
    let data = build_texture_data::<TEXTURE_GRADIENT_STEPS>(width, &colors.positions, greyscale, &colors.values);
    let data_bytemuck = box_bytes_of(data);

    asset_manager.add_asset_data(
        path,
        AssetData::Texture(TextureData {
            info: TextureInfo {
                dimension: TextureDimension::Dim2D,
                width,
                height: 1,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::INITIAL_COPY,
                supports_srgb: false,
                format: if greyscale { Format::R8UNorm } else { Format::RGBA8UNorm }
            },
            data: smallvec![data_bytemuck]
        }),
        AssetLoadPriority::High
    );
}


fn build_texture_data<const STEPS: usize>(width: u32, positions: &[f32; STEPS], greyscale: bool, colors: &[[f32; 4]; STEPS]) -> Box<[u8]> {
    let components: usize = if greyscale { 1 } else { 4 };
    let mut data = Vec::<u8>::with_capacity((width as usize) * components);
    let mut start_pos_index = 0usize;
    for i in 0..width {
        let pos = (i as f32) / (width as f32);
        if start_pos_index != STEPS - 2 && pos >= positions[start_pos_index + 1] {
            start_pos_index += 1;
        }
        let start_position = positions[start_pos_index];
        let end_position = positions[start_pos_index + 1];
        let start_color = colors[start_pos_index];
        let end_color = colors[start_pos_index + 1];

        let lerp_pos = (pos - start_position) / (end_position - start_position).min(0.0).max(1.0);
        for j in 0..components  {
            let mut color_component_float = start_color[j] * (1.0f32 - lerp_pos);
            color_component_float += end_color[j] * lerp_pos;
            let color_component_u8 = (color_component_float * 255.0f32) as u8;
            data.push(color_component_u8);
        }
    }
    data.into_boxed_slice()
}

