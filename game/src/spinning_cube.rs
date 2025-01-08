use std::f32;
use std::marker::PhantomData;
use std::sync::Arc;

use bevy_app::{App, Plugin, SpawnScene};
use bevy_ecs::component::Component;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Commands, Query, Res, ResMut, Resource};
use bevy_input::keyboard::KeyCode;
use bevy_input::ButtonInput;
use bevy_transform::bundles::TransformBundle;
use bevy_transform::components::Transform;
use bevy_log::*;
use sourcerenderer_core::input::Key;
use sourcerenderer_core::{
    Platform, PlatformPhantomData, Quaternion, Vec2, Vec3
};
use sourcerenderer_engine::Engine;

use sourcerenderer_engine::asset::{
    AssetManager,
    MeshRange,
};
use sourcerenderer_engine::camera::ActiveCamera;
use crate::fps_camera::FPSCameraComponent;
use sourcerenderer_engine::math::BoundingBox;
use sourcerenderer_engine::renderer::{
    PointLightComponent,
    StaticRenderableComponent,
    Vertex,
};
use sourcerenderer_engine::Camera;

pub(crate) struct SpinningCubePlugin<P: Platform>(PlatformPhantomData<P>);

impl<P: Platform> Default for SpinningCubePlugin<P> {
    fn default() -> Self { Self(Default::default()) }
}

#[derive(Component)]
struct SpinningCube {}

unsafe impl<P: Platform> Send for SpinningCubePlugin<P> {}
unsafe impl<P: Platform> Sync for SpinningCubePlugin<P> {}

impl<P: Platform> Plugin for SpinningCubePlugin<P> {
    fn build<'a>(&self, app: &'a mut App) {
        app.insert_resource(PlaceLightsState { was_space_down: false });
        app.add_systems(SpawnScene, (place_lights, spin::<P>,));

        {
            let asset_manager: &Arc<AssetManager<P>> = Engine::get_asset_manager(app);

            let indices: [u32; 36] = [
                2, 1, 0, 0, 3, 2, // front
                4, 5, 6, 6, 7, 4, // back
                10, 9, 8, 8, 11, 10, // top
                12, 13, 14, 14, 15, 12, // bottom
                18, 17, 16, 16, 19, 18, // left
                20, 21, 22, 22, 23, 20, // right
            ];

            let triangle = [
                // FRONT
                Vertex {
                    position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, -1.0f32),
                    uv: Vec2::new(0.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
                    normal: Vec3::new(1.0f32, 0.0f32, -1.0f32),
                    uv: Vec2::new(1.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, -1.0f32),
                    uv: Vec2::new(1.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, -1.0f32),
                    uv: Vec2::new(0.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                // BACK
                Vertex {
                    position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
                    uv: Vec2::new(1.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
                    uv: Vec2::new(0.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
                    uv: Vec2::new(0.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, 0.0f32, 1.0f32),
                    uv: Vec2::new(1.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                // TOP
                Vertex {
                    position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, 1.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                // BOTTOM
                Vertex {
                    position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
                    normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
                    normal: Vec3::new(0.0f32, -1.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                // LEFT
                Vertex {
                    position: Vec3::new(-1.0f32, -1.0f32, -1.0f32),
                    normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, 1.0f32, -1.0f32),
                    normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, 1.0f32, 1.0f32),
                    normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(-1.0f32, -1.0f32, 1.0f32),
                    normal: Vec3::new(-1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                // RIGHT
                Vertex {
                    position: Vec3::new(1.0f32, -1.0f32, -1.0f32),
                    normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, 1.0f32, -1.0f32),
                    normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 0.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, 1.0f32, 1.0f32),
                    normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(0.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
                Vertex {
                    position: Vec3::new(1.0f32, -1.0f32, 1.0f32),
                    normal: Vec3::new(1.0f32, 0.0f32, 0.0f32),
                    uv: Vec2::new(1.0f32, 1.0f32),
                    lightmap_uv: Vec2::new(0.0f32, 0.0f32),
                    alpha: 0f32,
                    ..Default::default()
                },
            ];

            /*let mut image_file = <P::IO as IO>::open_asset("texture.png").expect("Failed to open texture");
            let length = image_file.seek(SeekFrom::End(0)).unwrap();
            image_file.seek(SeekFrom::Start(0)).unwrap();
            let mut data = Vec::<u8>::with_capacity(length as usize);
            unsafe {
            data.set_len(length as usize);
            }
            image_file.read_exact(&mut data).unwrap();
            let image = image::load_from_memory(&data).unwrap();
            let data = image.to_rgba8();
            let texture_info = TextureInfo {
            format: Format::RGBA8,
            width: image.width(),
            height: image.height(),
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
                supports_srgb: false
            };*/

            let triangle_data = unsafe {
                std::slice::from_raw_parts(
                    triangle.as_ptr() as *const u8,
                    std::mem::size_of_val(&triangle[..]),
                )
            }
            .to_vec()
            .into_boxed_slice();
            let index_data = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u8,
                    std::mem::size_of_val(&indices[..]),
                )
            }
            .to_vec()
            .into_boxed_slice();
            let bounding_box =
                BoundingBox::new(Vec3::new(-1f32, -1f32, -1f32), Vec3::new(1f32, 1f32, 1f32));
            asset_manager.add_mesh_data(
                "cube_mesh",
                triangle_data,
                triangle.len() as u32,
                index_data,
                vec![MeshRange {
                    start: 0,
                    count: indices.len() as u32,
                }]
                .into_boxed_slice(),
                Some(bounding_box),
            );
            //asset_manager.add_texture("cube_texture_albedo", &texture_info, data.to_vec().into_boxed_slice());
            asset_manager.add_material_data("cube_material", "cube_texture_albedo", 0f32, 0f32);
            asset_manager.add_model_data("cube_model", "cube_mesh", &["cube_material"]);
        }

        app.world_mut().spawn((
            StaticRenderableComponent {
                receive_shadows: true,
                cast_shadows: true,
                can_move: true,
                model_path: "cube_model".to_owned(),
            },
            Transform::from_translation(Vec3::new(0f32, 0f32, 5f32)),
            SpinningCube {},
            /*RigidBodyComponent {
                body_type: RigidBodyType::Dynamic,
            },
            ColliderComponent::Box {
                width: 1f32,
                height: 1f32,
                depth: 1f32,
            },*/
        ));

        app.world_mut().spawn((
            Transform::from_translation(Vec3::new(0f32, -2f32, -5f32)),
            /*RigidBodyComponent {
                body_type: RigidBodyType::Static,
            },
            ColliderComponent::Box {
                width: 10f32,
                height: 0.1f32,
                depth: 10f32,
            },*/
        ));

        let camera = app.world_mut().spawn((
            Camera {
                fov: f32::consts::PI / 2f32,
                interpolate_rotation: false,
            },
            Transform::from_translation(Vec3::new(0.0f32, 1.0f32, -1.0f32)),
            FPSCameraComponent::default(),
        )).flush();

        app.insert_resource(ActiveCamera(camera));
        info!("Added spinning cube");
    }
}

fn spin<P: Platform>(mut query: Query<&mut Transform, With<SpinningCube>>, delta_time: Res<bevy_time::Time>) {
    for mut transform in query.iter_mut() {
        transform.rotation *= Quaternion::from_axis_angle(
            Vec3::new(0.0f32, 1.0f32, 0.0f32),
            1.0f32 * delta_time.delta_secs(),
        );
    }
}

#[derive(Resource)]
pub struct PlaceLightsState {
    was_space_down: bool,
}

fn place_lights(
    input: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<PlaceLightsState>,
    query: Query<(&mut Transform,)>,
    mut commands: Commands
) {
    if input.pressed(KeyCode::Space) {
        if !state.was_space_down {
            for (transform,) in query.iter() {
                commands.spawn(
                    (Transform {
                        translation: transform.translation,
                        rotation: Quaternion::default(),
                        scale: Vec3::new(1f32, 1f32, 1f32),
                    },
                    PointLightComponent { intensity: 1.0f32 },)
                );
            }
        }
        state.was_space_down = true;
    } else {
        state.was_space_down = false;
    }
}
