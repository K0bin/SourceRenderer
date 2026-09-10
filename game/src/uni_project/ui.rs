use bevy_app::{App, Plugin, Update};
use bevy_ecs::change_detection::{NonSendMut, Ref, ResMut};
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res};
use sourcerenderer_engine::DearImgui;
use sourcerenderer_engine::dear_imgui_rs::{ChildWindow, Condition, ListBox};
use sourcerenderer_engine::renderer::VolumeMeshInstance;

pub(super) struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UIState::default());
        app.add_systems(Update, (volume_meshes_ui_system,));
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
                        .size(ui.content_region_avail())
                        .build(ui, || {
                            for (entity, mesh) in &instances {
                                let text = format!("Mesh {:?}", mesh.threshold_min);
                                if ui
                                    .selectable_config(&text)
                                    .selected(state.selected == Some(entity))
                                    .build()
                                {
                                    state.selected = Some(entity);
                                }
                            }
                        });
                });

            ui.same_line();
            ChildWindow::new("##meshproperties").build(ui, || {
                if let Some(entity) = state.selected {
                    let (_, mut mesh) = instances.get_mut(entity).unwrap();
                    ui.text("Min Threshold:");
                    ui.set_next_item_width(ui.content_region_avail_width());
                    ui.slider(
                        format!("##minthreshold{:?}", entity),
                        0.01f32,
                        1.0f32,
                        &mut mesh.threshold_min,
                    );

                    ui.text("Transparent:");
                    ui.same_line();
                    ui.checkbox(format!("##transparent{:?}", entity), &mut mesh.transparent);
                }
            });
        });
}
