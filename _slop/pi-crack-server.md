we have implemented separate customizable stages for our plans, but when refreshing the page we don't see the previous questions and their answers. We should display those if they exist above the plan summary in the plan file. 

Each stage (explore, plan) currently flows on the single task page one after the other. Let's instead have some tab buttons (that we make in css sticky to the top of the page, so we can jump around) where we only show one of these stage trajectories. When they are running, any received message will jump it if it is not currently opened. It should also scroll to the message and highlight the message div with a light green alpha 30% border 1px dotted. The task tabs will be colored green when current, blue when done, and grey when not yet started/disabled. Use the "color" attribute in the css element. 

Add a third stage s03_plan_review.py which will run a different second model (default nvidia/z-ai/glm-5.2) which will have some prompts that tell it the original user prompt, the explore summary, the plan read from disk resulted from the previous phase, and this new session agent using glm is now told to criticize the plan and ask a second round of user grilling - one round of a few questions, then if needed a second round of questions as follow-up after the first round. Using these, the plan is then updated by the agent itself (give it the plan path) and then ask the user in the server UI if the plan is approved. If it is not approved, the UI will ask the user to comment on what's wrong (multiline input) and send that to the plan critic agent chat to adjust the plan some more. If the plan is approved, set the plan as "approved" in the metadata (show a blue checkmark in the top part of the page and in the list home page instead of a gray circle). The server page should display the whole trajectory of user questions as well as the other traffic happening. 

Every time the plan was changed in one of these loop iterations, we run a step that asks the glm model to write or update the staged todo file. This is a file at a path just next to the plan.md file inside the task dir, where we write a list of - [ ] pending_item or - [x] done_item - we regenrate this file from the plan file using a single pass of the nvidia ultra model (name this the todo generator model). This todo generator model will regenerate the todo every time it's done.

Besides the "approve" and "reject with written reason" modes, we also have the "grill me some more about ..." button which will spawn directly another round of questions using the glm model.


Make sure every time the user is asked a multiple-choice or single-choice question, they also get an option that says "Other, " and then has an input box besides it where the user can fill in a completely different choice. This is also multiline (let's say at 2 lines default) and is non-disabled only appears when other checkbox/radiobutton is picked. 

When the plan is approved by the user, jump into the next s04_implementation.py stage where we don't actually run any AI, we just display a big markdown->html subsection with the prompt:
- the original user prompt
- the exploration summary
- the final plan from previous stage: the verbatim plan text + the plan file path
- the todo stage: the verbatim todo text + the todo file path + instructions to update the todo after every single implementation change
- mention that we are now implementing the code and now make the changes, we have left plan mode. 

