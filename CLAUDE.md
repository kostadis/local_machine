This directory will contain scripts and tools that i need to manage this particular virtual machine (see ~/src/CLAUDE.md).

Having said that the specifics of the particular machine (ip address, etc) will be kept as configuration files, and not baked into the scripts. 

The configuration files will use yaml.

Configuration is stored in ~/.config/local-machine by default. The location is a parameter (`--config-dir`), shared by every sub-command that reads configuration.

The tools will assume an ubuntu linux system. 

There will be a local repo

To make discovery of tools easy, the system will be structured so that there is a top-level command called "local-machine" that all sub commands will derive from. 

Commands take the form `local-machine <subcommand> <action>`, e.g. `local-machine export start`. A subcommand is a capability; its operations are actions (verbs) under it, not sibling subcommands or flags. Actions with the same meaning use the same verb across subcommands (e.g. start / status / stop).

To make life simple, any sub-command that needs a same parameter will use the same parameter name, we will not invent new parameters for new commands. 

The code of this tool will be rust. 
 
