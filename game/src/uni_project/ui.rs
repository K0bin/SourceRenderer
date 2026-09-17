use crate::uni_project::{MANIX_PATH, TRANSFER_FUNCTION_PATH, manix_transform};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::change_detection::{NonSendMut, ResMut};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Commands;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::Query;
use bevy_math::Affine3A;
use sourcerenderer_engine::dear_imgui_rs::{ChildWindow, Condition, ListBox};
use sourcerenderer_engine::renderer::{VolumeMeshInstance, VolumeRendererOptions};
use sourcerenderer_engine::transform::InterpolatedTransform;
use sourcerenderer_engine::{DearImgui, VolumeDrawableTransparencyMode};

pub(super) struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UIState::default());
        app.add_systems(Update, (volume_meshes_ui_system,));
        app.add_systems(Update, (renderer_options_ui_system,));
    }
}

#[derive(Default, Resource)]
struct UIState {
    selected: Option<Entity>,
}

fn volume_meshes_ui_system(
    imgui: NonSendMut<DearImgui>,
    mut instances: Query<(Entity, &mut VolumeMeshInstance)>,
    mut state: ResMut<UIState>,
    mut commands: Commands,
) {
    let ui = imgui.ui();
    let window_size = [500.0f32, 400.0f32];

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
                            ui.content_region_avail()[1] - 64.0f32,
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
                        let new_entity = commands
                            .spawn((
                                VolumeMeshInstance {
                                    volume_texture_path: MANIX_PATH.to_string(),
                                    volume_texture_lod: 3,
                                    transfer_function_texture_path: TRANSFER_FUNCTION_PATH
                                        .to_string(),
                                    threshold_min: 0.95f32,
                                    transparent: VolumeDrawableTransparencyMode::Opaque,
                                    render_as_cubes: false,
                                },
                                InterpolatedTransform(Affine3A::from_mat4(manix_transform())),
                            ))
                            .id();
                        state.selected = Some(new_entity);
                    }
                });

            ui.same_line();
            ChildWindow::new("##meshproperties").build(ui, || {
                if let Some(entity) = state.selected {
                    if let Ok((_, mut mesh)) = instances.get_mut(entity) {
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

                        if ui.button("Delete mesh##addmeshbutton") {
                            commands.entity(entity).despawn();
                            state.selected = None;
                        }
                    }
                }
            });
        });
}

fn renderer_options_ui_system(
    imgui: NonSendMut<DearImgui>,
    mut options: ResMut<VolumeRendererOptions>,
) {
    let ui = imgui.ui();
    let window_size = [250.0f32, 100.0f32];

    ui.window("Renderer options##volumerenderer")
        .size(window_size, Condition::FirstUseEver)
        .build(|| {
            ui.text("Ray march normals:");
            ui.same_line();
            ui.checkbox("##raymarchnormals", &mut options.ray_march_normals);
        });
}
