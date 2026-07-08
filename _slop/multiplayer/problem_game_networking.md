i joined the game from two native clients and while the old globalchat works, the new multiplayer system doesn't seem to work at all anymore, i don't see each other player's data, i see this on the debug window



Configuration
Send Hz

Statistics
Connection: Connected
game room: joined
Sent: 540 msgs (20.04 KB) — 16.0 msgs/s
Received: 0 msgs (0.00 KB) — 0.0 msgs/s
Duplicates Dropped: 0
Decode Errors: 0
Channel-Full Drops: 0

Connected Peers
No other players connected.


and i see this on the logs for the client:


2026-07-08T14:28:03.178449Z  INFO net_crackpipe::global_matchmaker: Successfully connected to foreign bootstrap node
2026-07-08T14:28:03.178474Z  INFO net_crackpipe::global_matchmaker: connect_bootstrap_chat()
2026-07-08T14:28:03.178479Z  INFO net_crackpipe::global_matchmaker: connect_global_chats(): joining normal chat
2026-07-08T14:28:03.218419Z  INFO demo_resolution_selector_web_bevy::plugins::crack_plugin::osm_flow: OSM Category waterways: 3 features
2026-07-08T14:28:03.226526Z  INFO demo_resolution_selector_web_bevy::plugins::geojson: GeoJSON loading is fully completed!
2026-07-08T14:28:03.246693Z  INFO demo_resolution_selector_web_bevy::plugins::traffic::road_graph: TrafficRoadGraph: starting build from GeoJsonDatabase...
2026-07-08T14:28:03.246965Z  INFO demo_resolution_selector_web_bevy::plugins::geojson: Successfully found Bus 335 route with 639 points! Initializing movement.
2026-07-08T14:28:03.250051Z  INFO demo_resolution_selector_web_bevy::plugins::traffic::road_graph: TrafficRoadGraph: built with 2875 segments and 4138 node junctions.
2026-07-08T14:28:03.364289Z  INFO net_crackpipe::global_matchmaker: connect_global_chats(): done.
2026-07-08T14:28:03.364526Z  INFO net_crackpipe::chat::chat_controller: wait_until_joined: found 1/2 nodes (room bootstrap: 2), attempts: 0
2026-07-08T14:28:03.543290Z  INFO net_crackpipe::chat::chat_controller: wait_until_joined: found 2/2 nodes (room bootstrap: 2), attempts: 1
2026-07-08T14:28:04.181313Z  INFO thread_worker: Checking bootstrap node: PublicKey(ee24a3dca85715720293e897858fcef6318209b813b2f551e08e1eba83650b94)
2026-07-08T14:28:05.682782Z  INFO thread_worker: Checking bootstrap node: PublicKey(317e7c276f5c7f4800539c7a1ebe771a5cb8f6f85d08f61123a7b227f3dcb27f)
2026-07-08T14:28:05.776272Z  INFO thread_worker: Found live bootstrap node: PublicKey(317e7c276f5c7f4800539c7a1ebe771a5cb8f6f85d08f61123a7b227f3dcb27f)
2026-07-08T14:28:05.776329Z  INFO thread_worker: At least one bootstrap node is alive. No action needed.
2026-07-08T14:28:05.776407Z ERROR iroh::magicsock: send relay: message dropped, channel to actor is closed node=317e7c276f relay_url=https://net2.sparganothis.org./
2026-07-08T14:28:06.544360Z  INFO net_crackpipe::chat::chat_controller: wait_until_joined: found 1/1 nodes (room bootstrap: 2), attempts: 0
2026-07-08T14:28:06.544575Z  INFO demo_resolution_selector_web_bevy::plugins::network: Starting bevy chat incoming loop...
2026-07-08T14:28:06.549519Z  INFO demo_resolution_selector_web_bevy::plugins::network: P2P network connected.
2026-07-08T14:28:06.549585Z  INFO demo_resolution_selector_web_bevy::plugins::network: Gameplay chat network connected.
2026-07-08T14:28:12.791223Z  INFO demo_resolution_selector_web_bevy::plugins::network: Bevy received message from Verbindungsbahnbrücke: TextMessage { text: "a" }
2026-07-08T14:28:23.373478Z  INFO net_crackpipe::global_matchmaker: added connection to bootstrap node #1
2026-07-08T14:28:26.371432Z  INFO net_crackpipe::global_matchmaker: Spawning new bootstrap endpoint #3
2026-07-08T14:28:26.764441Z  INFO net_crackpipe::chat::direct_message: creating message dispatchers dict
2026-07-08T14:28:26.764703Z  INFO net_crackpipe::global_matchmaker: Connecting to own bootstrap endpoint
2026-07-08T14:28:27.829650Z  INFO net_crackpipe::global_matchmaker: added connection to bootstrap node #3
2026-07-08T14:28:29.767091Z  INFO net_crackpipe::global_matchmaker: Successfully connected to own bootstrap endpoint
2026-07-08T14:28:29.767149Z  INFO net_crackpipe::global_matchmaker: global periodic task: spawned new bootstrap endpoint
2026-07-08T14:28:29.767162Z  INFO net_crackpipe::global_matchmaker: connect_bootstrap_chat()
2026-07-08T14:28:29.773746Z  INFO net_crackpipe::global_matchmaker: run_bs_global_chat_task
2026-07-08T14:28:31.406323Z  INFO game_logic::lod: 0 split requests / 0 merge requests. compute_lod_changes took 14 ms
2026-07-08T14:28:42.240556Z  WARN calloop::loop_logic: [calloop] Received an event for non-existence source: TokenInner { id: 3, version: 11, sub_id: 0 }
2026-07-08T14:28:45.108476Z  WARN calloop::loop_logic: [calloop] Received an event for non-existence source: TokenInner { id: 3, version: 15, sub_id: 0 }
2026-07-08T14:28:45.440313Z  INFO game_logic::lod: 0 split requests / 0 merge requests. compute_lod_changes took 13 ms
2026-07-08T14:28:49.256068Z  INFO game_logic::lod: 0 split requests / 0 merge requests. compute_lod_changes took 13 ms
2026-07-08T14:28:53.220975Z  WARN calloop::loop_logic: [calloop] Received an event for non-existence source: TokenInner { id: 3, version: 30, sub_id: 0 }
2026-07-08T14:28:57.538075Z  INFO game_logic::lod: 0 split requests / 0 merge requests. compute_lod_changes took 13 ms
2026-07-08T14:28:59.228951Z  INFO demo_resolution_selector_web_bevy::plugins::network: Bevy received message from Verbindungsbahnbrücke: TextMessage { text: "a" }
2026-07-08T14:29:21.505664Z  INFO bevy_window::system: No windows are open, exiting
2026-07-08T14:29:21.505752Z  INFO bevy_winit::system: Closing window 67v0


everything seems in order, however the multiplayer is not working anymore at all ! please review the code again and see why that is. the servers don't agree on 



------

I know what it is ! In iroh, you connect to a chat room by both its topic and at least one peer address. Since we don't have any peer address for the other player if they both join at the same time, we can't join the same network. Let's instead make all the bootstrap node threads also join the gameplay chat in their respective loops and make it so that they drop all the traffic (or make a statistic of it and print it every 10min). So each client connects to themselves only. This is not what we want. 

Then, when joining the gameplay chat network topic, we add all the bootstrap nodes (all 5) to the ticket and both players will connect to the same room so we can play multiplayer. The bootstrap nodes themselves, when connecting to the gameplay network room. The bootstrap nodes, when connecting to the gameplay room, will add to the ticket list the ids of all the nodes that own those bootstrap nodes. Same goes for when we're connecting as a normal user.

FInally, refactor the code such that all this logic of spawning a thread for the bootstrap, connecting the bootstrap to the correct networks, etc. All this should go in a global init function in the network crate, and we should only call the init function from the correct places to get a single "network manager" like object that contains all our running code. 
