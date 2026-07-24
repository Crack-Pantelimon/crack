Let us investigate the possibility of using podman's overlay container feature to sandbox each separate harness conversation (both agents and sub-agents) using the :O container mount feature, including separate forks of both the workspace dir mount , and forking the target volume mount, and forking the root fs mount.

### Basic Idea

We need to sandbox each one of our conversations against each other, as they break things before they're done, and having multiple online is very chaotic. Each agent and sub-agent will thus receive its own container with copy-on-write over all of the volumes, and will generate a git commit patch file with its final changes. This commit patch file is returned to the caller (in case of a sub-agent sub-agent) so they can apply the patch (add instructions for the agent calling the sub-agent on how to actually do that properly). 

Each chat conversation and each sub-agent runs in its own container where all the volumes are temporary copy-on-write over the crack-dev container. 

The crack-dev container is the only one that will host the pi/crack/server server and worker. It will only run "pi" subcommands for summarization and other small tasks using the small models, all agentic traffic must happen in sub-containers. The sub-containers will be alive for the duration of their conversation, they will host their own clones of all the mcp tools including browsers and blender, and will be running their respective "pi" subprocess runs. The root container, thus, instead of running "pi" in a subcommand, will be running "docker exec crack-sandbox-123 .... pi ...." as a subcommand. 

Each top-level conversation (as opposed to sub-agents) will do the same git add / commit patch file creation logic, but will now instead do something else with the result. They will apply the commit file if it exists.

If a sub-agent or top-level agent does not actually change any code, it will not be nagged about producing any diffs - it must have been some exploration/read-only task that did not require code change.

#### Bringing back changes from sub-containers

After the normal agent/sub-agent trajectory is finished, we use git to check the workspace tree for changes, and do "git add ."  on the entire workspace. 

We then verify using python each changed file to ensure it is under the max allowed size of 95.0MB per file. If any file is bigger, then unstage all of the files, and nag the agent that finished to ensure its big files are ignored in gitignore , list and point out which files are big with their full paths, and let the agent add them to gitignore or clean them up, then try again, to a maximum of 5 git add attempts.  If after 5 retries the git commit is still containing files that are too big, simply run "git add" on all files except for the big ones, return that diff.


#### Sub-container details

Right now, all MCP tooling is started on container start, including expensive things like browsers and blender, and that causes the docker image to require 10gb ram. We should instead only start those servers when a MCP client explicitly activates that feature - is that possible with our current plugins? Otherwise, the containers take 10gb ram each, so they take up too much ram. 

The root container "crack-dev" will have new mounts added in _docker/run.sh and new packages installed in Dockerfile such that we can run docker inside the image, to control the docker from the host. We are running podman, so that probably means adding podman inside the container (base image is debian, see Dockerfile.base) and then connecting some socket or something similar to that.

The sub-containers ("sandboxes") will not expose any ports, but they will have available inside the mcp tools should they require them, and these will be isolated from the other sandboxes.

The "root" container will also have acces to all these mcp tools in case the user logs in and wants to debug with "pi".

Each conversation trajectory will start its container once with command "sleep 10000000" and exec pi commands into it using "docker exec".

Stopping the chat means killing the container.

Docker container management will be similar to pi process management (or completely replace it): when task is stopped, normally or forcefully, do the git add (if stop was forceful, just skip the big files) and add it to the output patch.  


#### Working on itself

We want the harness to be able to work on itself. This is not currently possible without sandboxes, since the agent will modify itself and will auto-reload and if the code is wrong and it crashes, it's dead now and cannot continue. Conceptually verify that this sandbox setup will allow us to work on itself.