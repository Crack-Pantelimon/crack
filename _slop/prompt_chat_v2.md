in this current patch we pulled code from _slop/examples/Sparganothis-v2/* code related to node identities,chat managers, presence managers, etc. into our package/net_crackpipe crate. to use the crate, we create a new binary crack_demo/demo_.../bin/chat.rs that displays the default scene and a full screen chat window that shows presence on the left and chat on the right and username in bottom left corner and chatbox in bottom right. the chat connects to a global chat and pastes messages there.


we have made these changes and the chat doesn't work: 

    node_id: PublicKey(789b8fd9b59e0c6ec367504f0ea452f7a7cc626f678aa19bf8883676f0106106),
    bootstrap_idx: None,
}
2026-07-07T17:19:23.539521Z  INFO ep{me=789b8fd9b5}:magicsock:actor: iroh::magicsock: home is now relay https://net2.sparganothis.org./, was None
2026-07-07T17:19:23.539569Z  INFO net_crackpipe::chat::direct_message: creating message dispatchers dict
2026-07-07T17:19:24.670508Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="10f61de4e5"}:discovery{me=789b8fd9b5 node=10f61de4e5}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:24.670603Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="7d4f0da52b"}:discovery{me=789b8fd9b5 node=7d4f0da52b}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:24.671404Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="317e7c276f"}:discovery{me=789b8fd9b5 node=317e7c276f}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:24.671577Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="cb717152a5"}:discovery{me=789b8fd9b5 node=cb717152a5}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:24.674559Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="ee24a3dca8"}:discovery{me=789b8fd9b5 node=ee24a3dca8}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:24.674694Z  INFO net_crackpipe::global_matchmaker: Spawning new bootstrap endpoint #2    
2026-07-07T17:19:25.210779Z  INFO ep{me=10f61de4e5}:magicsock:actor: iroh::magicsock: home is now relay https://net2.sparganothis.org./, was None
2026-07-07T17:19:25.210823Z  INFO net_crackpipe::chat::direct_message: creating message dispatchers dict
2026-07-07T17:19:25.211348Z  INFO net_crackpipe::global_matchmaker: Connecting to own bootstrap endpoint  
2026-07-07T17:19:25.285037Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="10f61de4e5"}:discovery{me=789b8fd9b5 node=10f61de4e5}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:25.285044Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="cb717152a5"}:discovery{me=789b8fd9b5 node=cb717152a5}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:25.285074Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="317e7c276f"}:discovery{me=789b8fd9b5 node=317e7c276f}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:25.285081Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="ee24a3dca8"}:discovery{me=789b8fd9b5 node=ee24a3dca8}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:25.285095Z  WARN connect{me="789b8fd9b5" alpn="sparganothis/global-matchmaker-echo/0" remote="7d4f0da52b"}:discovery{me=789b8fd9b5 node=7d4f0da52b}: iroh::discovery: discovery service produced error err=Resolve request failed with status 404 Not Found
2026-07-07T17:19:25.285520Z  WARN net_crackpipe::global_matchmaker: failed to create global matchmaker, retrying 2/3... failed to connect to any bootstrap node
2026-07-07T17:19:40.721612Z  INFO bevy_window::system: No windows are open, exiting
2026-07-07T17:19:40.721739Z  INFO bevy_winit::system: Closing window 67v0




---------------

investigate the differences between the source implementation under sparganothis and our clone. the problem seems to be the 404 error, but we can see the url just fine in the browser and both urls are correct


Write a plan into _slop/plan_chat_v2.md where with findings and explanations into why the proposed fix will work, as well a a technical spec of the full solution - we will review this plan before we continue.