- pretty display

The "Turn 1 .. Turn 11" display cards should have their data in a more friendly format, like 

 read: {'path': '/workspace/crack_demo/demo_resolution_selector_web_bevy/src/plugins…

 should actually truncate the middle part of the path, not the end part, cuz we don't know what file we're talking about.

 - The bash commands should be shown in full multiline in pre section
 - the read sections should show the path truncated in the middle and truncate the file after 200 lines  or 10,000 characters and show the agent a friendly truncation mark, and instruct it to read specific lines if needed. 
 - add to the explore prompt that it can also use "sigmap ask 'Where is the sky implemented?'" to get referenced function headers for some questions the model will consider it has on the phone. It will use bash sigmap to get those references. 

 Referenced files
workspace/src/lib.rs

Could not resolve workspace/src/lib.rs under the project root.


As you can see, the referenced files are treated as relative when they are absolute. Any absolute paths that start with the project dir will have their prefix stripped. Any paths that are relative will also be checked for existance. Only existing paths will be shown in the bottom referenced files section.

The summary shuold be displayed using markdown->html change (use uv add inside the server dir insinde the container) to render properly online.

There's a bug with "regenerate title" where clicking the button will replace the whole title section with only the edit box, so the title and buttons all dissapear. review the htmx id scheme and figure out what's wrong there too. 

Show the turns above the Explore summary. Show them compressed - instead of cards, let's have a huge compressed table with the type on the left, paths/codes in the middle, and some statistics on the right too like input/output size.



The explore turns are not serialized at all, and neither is the summary of the exploration - we should store all this data in .pi/crack/tasks/<task_id>/explore/<artefact_name>.md - so for the explore summary, the artefact name would be explore_summary. The turns will be encoded in .pi/crack/tasks/<task_id>/explore.json as well as the reference paths after filtering. Refreshing the page will read these files, else the section is unpopulated until we click on explore. Keep a metadata explored_at timestamp and a prompt_last_modified_at timestmap and we can compare the two to figure out if we want to re-run explore again. 

Add a "turn zero" before the 15 turns where we ask the model to write down between 2 and 10 questions about what kind of code we want to find, and then to generate/hallucinate some example answers to each one in the same turn . This text will be added to the other turns, so it might be useful to take an additoinal turn to try and fill out as much of the search space as possible going forward. This will be of course another prompt in the template folder. 

I see our exploration takes all 15 turns, we should invite the model to early stop and tell it how to do so (by looking at the pi documentation online)

Research online on running cheap and effective exploration sub-agents, and take that into consideration when suggesting some system improvements that we will also implement on this turn to better implement exploration and more effective that stops earlier. 

We should also consider splitting the 15 turn sequence into shorter hops of maybe 3 or 5 turns each, then another retrospective chat-only request as in "what else should we look at that we had on the list" repeated a few times until it says enough and stops.

When exploration is done, also keep a metadata of when it ended, so we can diff between the first metadata and show what time it took and how many turns. Truncated because max turns or max time exploration is still valid exploration if it found at least 1 relevant file.