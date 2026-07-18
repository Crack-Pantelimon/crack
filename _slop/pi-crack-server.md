under .pi crack server tools in the other thread, we do too many git commits - we commit every single turn. instead, we should commit when each stage is done only - not when each turn is done. That should cut down on the commit number. 

The implementation review screen for this task is missing its trajectory with the read / think history. See this link : 

http://localhost:9847/tasks/1784383061217_clouds_skybox_lighting_add_color_temperature_slider/view/impl_review

Whenever we wait for a number of turns to complete for a turn cap, we should NOT increment the turn count for each tool call separately. We should increment only once for an entire group of tool calls. We are expiring 60 turns where most of them are reading files. 

In the home page, under the # Harness Stages section, add a vertical line, and then show a second category beneath "Unscripted Chats" with a list of the most recent chats. You can click on one of the older chats or click to expand the full list (sorted by time decreasing) and their id is timestamp in milliseconds. Or you can click the "New Chat" button that will be over the recent chats just under the title. Creating a new chat will redirect into its chat page. The chat input will be at the bottom a multiline input, a dropdown to select from cached models, and a "send" key. All tools should be available to the agent here, including the new ones listed below. Each chat will be stored on disk in json files under .pi/crack/unscripted_chats/<chat_id>/<item>.json .


Then, we want to add some additional tools inside the docker container definition _docker/Dockerfile and into the live container here : `docker exec crack-dev rg --version`. First, install the softwares into the live container to confirm they work, then put those exact steps back into the dockerfile build steps, be it installation of packages, of debs, etc. We prefer downloading the packages using deb files. Remember we are on debian 13. Do not build the container, just change the dockerfile. Here is the types of softwares we want installed and enabled like explained above:
- chromium and firefox
- compatible geckodriver and chromedrivers
- wasm-pack: cargo install wasm-pack (to test using wasm each test suite)
- https://pi.dev/packages/pi-mcp-adapter  -- $ pi install npm:pi-mcp-adapter
- mcp servers for the two browsers and drivers we have installed ; both should be available through the pi mcp adapter, so write the configuration required in our repo
- mcp server for Web search mcp: https://github.com/mrkrsl/web-search-mcp - clone this into /root/web-search-mcp using fixed commit eeb03f88525cbf74c4019e59a3fea45a537a760b
  and set it up to start at container boot, and set it up to be available for all places where tools called are allowed (most of the agents and the unscripted chats)

All of these softwares in the container are going to be set up to be used by the main agent - so check yourself that all these work properly by using "pi" with the nvidia ultra model to interact with each one of them and test what configurations work and what don't - for example for the 3 types of web searches, run a search about the weather in las vegas usa and see what of the various methods return the proper content - and take note of all these in a README.md file in the code.