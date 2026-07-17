In .pi/crack/server

- now, each prompt has "regenerate title" button and title section, but we instead want the only "title" to be the one for the whole section (the top title). So there is onely one title, and the "Regenerate Ttitle" comes in between the title edit box and the save button.
- The Regenerate title button is currently not working. We want to use a prompt template together with "pi" cli tool to run using gemma is currently not working at all. Let's add more logggin and timeouts and information to the call when we call it. We should log: the full prompt, the output summary, the timeouts involved, the command line with + in front of it when we run pi, the time taken for a result. Then, test out inside the container that the pi command line works by using docker exec -it crack-dev /bin/bash -exc "pi --version" to run pi. Check that we can summarize things like that. 


Write a full implementation change plan for a lesser model to run into _slop/pi-crack-plan.md