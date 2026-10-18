# idea

the idea is having agent(s) access their memory,plans, agent definitions, scripts etc. as a "browser" with sites as the folder structure.

every "site" has an index.md which represents the homepage of said site.
every site can have a `justfile` allowing them to adapt and use scripts that that site builds.
every site can have a human facing UI build in svelte.

the application is split in 3 parts.
1. the TUI, this is the main interface i will use to chat with the agent, the agent will then be able to got through the sites and stuff like it.
2. micro apps build in svelte. these micro apps can be opened by the TUI or the server process. auto opened in a browser.
3. the harness / server. this will run the agent, have the session(s) open. 

everything is text first in this project. any page can link to another page, even accross sites, kinda like an actual browser.

---
how do sub agents work in this project. sub agents work by just having another instance of the server open. the orchestrator agent can make a ticket in the tickets site. or we can call it the orchestration site if we later want stuff like pipelines in there (which would be cool). 

---
language for the server and TUI is rust, the TUI is written with Ratatui. 

---
