we have a webserver under .pi/crack/server that is currently a little bit bugged:
- the "add prompt" and "edit prompt" form should also show filename.
- all the prompts should always be viewable from the tasks item at all times. they can be sent empty, in which case we try filenames prompt2..prompt9.md in order. The edit button in each item will only change from non-editable to editable version.
- all created task ids start with the integer milisecond tiemstamp and then the user title with non-alphanum replaced with _
- use the title "Crack Task: ... " on the sub-page for the tasks
- the "or specify custom name" button is very confusing, remove its functionality and instead just use the main form . also i think the ui is not replacing the things it thinks it's replacing - review for htmx errors.
- review that the code paths for the ui and backend interaction and 
- all save and edit buttons fail to work - review the server code please


in the task list home page:
- the create task button should add the timestamp to the id as a prefix by default


The server is running at all times from the docker container and is availabel at localhost:9847 for curl review. Add technical details that a model might want to know to the AGENTS.md server file at the very top of the file (above the spam with the code references) at this file: .pi/crack/server/AGENTS.md - and add any other things a lesser model running on a new session might want to know when working on these files. 