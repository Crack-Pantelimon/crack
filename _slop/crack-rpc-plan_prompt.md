crack_demo/web_frontend is a test frontend in dioxus that used the crack library to set up usage examples. the crack library is meant to allow building an app that uses web workers in an ergonomic fashion with code that also works on desktop with no changes. we want to use it to run network i/o, disk i/o, and facilitate rpc call like functionality for calling into expensive code from the frontend where it will be managed through async.

we first need to to create a new crate that will hold our game logic code under crack_demo/game_logic - this crate will declare a model group and an api group and then register both in crack_demo/web_worker and in the main game crate (include by path). this game_logic crate will add the crack pages dependency it needs (mainly the api and the storage crates) and then in the main game add a plugin called "CrackPlugin" that will manage async tasks.

Then move the parquet crate from the main game into the game_logic crate and move over the code for reading the map manifest into a new async function declared through the crack library. Finally, use bevy async tasks to move over the map manifest reading into the crack async logic.

On the worker side, the api implementation for fetching the map parquet manifest (and pre-processing it into a final struct) will happen only once - store it in a global tokio::sync::RwLock 

Then do the same for the OSM data - make a new api implementation to fetch and parse it and store it in the worker ram. Cache this into a rwlock too.

Finally, move the cpu-intensive part of the function recompute_lod_mark_changes function into another api function call. This function should receive the reference points and LOD settings, should return the splits, nodes, and other graph info required by the algorithm. Since this will happen async, make sure we only have one of these tasks active at any time, and we don't fire off more according to that system's logic. 

On desktop, make sure to pull and instantiate the crack_demo/thread_worker crate where we will also register the api group and the model group from our game logic.