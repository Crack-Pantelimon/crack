Install blender inside the container (you are root, just find the blender deb for latest version 5.1.* and install that)

when you have the correct download location , add that info to /workspace/_docker/Dockerfile so we have it in next containers also. 

Then install and set up using the next http port down, the blender mcp server . Configure it in the root dir /workspace the same as the other mcp servers . Blender will be running headless in the container, and the mcp should be able to control it . Verify that the mcp works by creating a blender file, replacing the basic cube with a sphere, and saving as /workspace/tmp/test.blend