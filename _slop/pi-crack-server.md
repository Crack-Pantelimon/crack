the Add Prompt form should show the prompt above the content, now they are side by side.

the Regenerate Title button also saves the new title. Also, any change of the prompt files where the actual content changed, we regenerate the title again. 

The Add Prompt form should be collapsed under smaller title font and only when expanded show the form and the Add button.

Under the prompts there is a new category: 

Below, under the add prompt form, add a new "explore" button. This explore button will use a second prompt template and paste in the concatenation of all the prompt files. The "explore" button will run a sub-agent using the same model for summarization. This model will take the prompts and try to find out where all the code relevant to the question might be, and print out the paths of those files complete with line number intervals at the bottom. Put the paths at the bottom and in the "Explore" section of the UI, show the explore result message by message as it happens - so use "pi" in its stdin/stdout jsonl communication mode and print out all the messages on the screen using a friendly format. Also run a regex match on the text returned by the explore answer and if they look like files under one of our code paths that the server can read at its root, then we display a collapsible with that reference and (if present and correct) the line range open in the code file. The explore agent should be stopped from running after 10 turns and all of its turns will be scanned for paths to show in the explore phase. After these maximum 10 turns, run a final turn to summarize the exploration into an overview using yet a third template.

Those 10 turns the agent should be allowed to use tools like "rg", 

The templates should all be moved into external files at .pi/crack/server/prompt_templates/<template_name>.md and we should use those templates intead of global strings inside the server code.

The container should have tools like  ripgrep, fzf, bat, eza, fd, zoxide, jq, tmux, lazygit, and more. Add them to the dockerfile at _docker/Dockerfile and then install them directly using command like docker exec -it crack-dev /bin/bash -exc "apt-get --version" to install using apt-get inside of the live container. Do not rebuild the container, just edit the package list and change the live one. 


