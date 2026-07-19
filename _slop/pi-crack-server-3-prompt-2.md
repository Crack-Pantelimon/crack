We have planned some changes in _slop/pi-crack-server-2-plan-1.md starting from our prompt at _slop/pi-crack-server-1-prompt-1.md . 


there have been some additional problems not mentioned in the plan above : 

we have been adding a server at .pi/crack/server that we have been coding on with multiple models. we have some problems with it : 

- the exact compiled prompts we send to the chat bot are not shown in the history . They should be all displayed in the trajectory as "user prompt" and expandable to show the original prompt. We need to see what goes in and also what goes out.

- whenever new messages appear in chat, while we do jump to the page and the last message, we can't expand any data because it immediately gets un-expanded since every 2s every refresh we seem to reset the page, even if there was no new content. There are also a lot of polling requests which are spamming the logs and server. We shold replace this behavior with long polling (to minimize request spam) and we should re-think the situation with the messages: new messages should be popped into place without resetting the rest of the page.

- Analyze the crack server codebase for extended duplication and make a plan to refactor this extended duplication into samller functions/modules/files. As a rule of thumb, functions should be under 100 lines, and files should be under 500 lines each.

- Analyze whe whole implementation and write a list of possible bugs not included in this file at path _slop/pi-crack-server-5-next-bugs.md


Analyze the previous plan files, the prompt, this prompt, and write out a new plan in parts pi-crack-server-4-plan-{1,2,3}.md that contains fixes for both everything mentioned above, and all the bugs mentioned previously into _slop/pi-crack-server-5-next-bugs.md - These 3 plan files should be independent , separately writeable and testable parts
of the implementation, each being able to be fed into a weaker sub-agent and implemented separately. Explore the codebase and write out our bugs file and our 3 plans files now . You can explore using the "docker exec crack-dev rg --version" inside the docker container to use a more rich palette of commands and you can also use the mcp tools inside there too (browser, web search, etc).