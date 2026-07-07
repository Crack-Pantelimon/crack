
Go to bevy
r/bevy
•
1y ago
AllenGnr
Tutorial - How to Load In-Memory Assets in Bevy

Hey everyone!

I recently ran into a situation where I needed to load assets that were generated at runtime into Bevy. While Bevy's AssetServer is great for loading assets from disk, and there's also the embedded_asset mechanism for bundling assets with your binary, there wasn't a straightforward way to load assets that are generated or modified during runtime.

After some digging, I discovered that Bevy has an internal MemoryAssetReader that can be used to load assets directly from memory. Although it was primarily designed for unit testing, it turned out to be exactly what I needed for my use case.

Here's how you can set it up:
Step 1: Define a Resource to Hold the Memory Directory

First, we need to create a resource to hold the Dir structure, which will store our in-memory assets.

use std::path::Path;
use bevy::{
    asset::io::{
        memory::{Dir, MemoryAssetReader},
        AssetSource, AssetSourceId,
    },
    prelude::*,
};

#[derive(Resource)]
struct MemoryDir {
    dir: Dir,
}

Step 2: Register the Memory Asset Source

Next, we need to register the MemoryAssetReader as an asset source. This allows the AssetServer to load assets from memory.

fn main() {
    let mut app = App::new();

    let memory_dir = MemoryDir {
        dir: Dir::default(),
    };
    let reader = MemoryAssetReader {
        root: memory_dir.dir.clone(),
    };

    app.register_asset_source(
        AssetSourceId::from_static("memory"),
        AssetSource::build().with_reader(move || Box::new(reader.clone())),
    );

    app.add_plugins(DefaultPlugins)
        .insert_resource(memory_dir)
        .add_systems(Startup, setup)
        .run();
}

Step 3: Insert Assets into the Memory Directory

Now, you can insert assets into the Dir structure at runtime. Here's an example of how to load a GLB file from disk into memory and then use it in your Bevy app:

fn setup(mut cmds: Commands, asset_server: ResMut<AssetServer>, mem_dir: ResMut<MemoryDir>) {
    cmds.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 7., 14.0).looking_at(Vec3::new(0., 0., 0.), Vec3::Y),
    ));

    cmds.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 10_000_000.,
            range: 100.0,
            shadow_depth_bias: 0.2,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 8.0),
    ));

    if let Ok(glb_bytes) = std::fs::read(
        "/path/to/your/asset.glb",
    ) {
        mem_dir.dir.insert_asset(Path::new("test.glb"), glb_bytes);

        cmds.spawn(SceneRoot(
            asset_server.load(GltfAssetLabel::Scene(0).from_asset("memory://test.glb")),
        ));
    }
}

Step 4: Use the In-Memory Asset

Once the asset is inserted into the Dir, you can reference it using the memory:// prefix in the AssetServer::load method. For example:

asset_server.load("memory://test.glb")

Conclusion

This approach allows you to dynamically load and use assets that are generated or modified at runtime, without needing to write them to disk. It's a powerful tool for scenarios where you need to work with assets that are created on-the-fly.

I hope this tutorial helps anyone else who might be facing a similar challenge. If you have any questions or suggestions, feel free to comment below!

Happy coding! 🚀 