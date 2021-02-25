# thingy
Lightweight build server and thing-doer

## Installation
`cargo install --force thingy`

## Usage
Run: `thingy path/to/workspace/folder`, then go to `http://localhost:8080/`

### Configuration
Thingy has few configuration options, which are provided as optional environment variables:

|Environment Variable|Default value| |
|-|-|-|
|`LISTEN_ADDRESS`|`127.0.0.1`|Address to bind web server to|
|`LISTEN_PORT`|`8080`|Port web server listens on|


Thingy works inside a 'workspace' folder. A thingy workspace is a plain folder with a `thingy.yaml` file in it. This file's structure is based on [this struct](./src/models.rs#L7). This file lists build jobs and configurations. If this file does not exist, an empty config with no jobs will be created. Jobs can then be added from web UI.

An example of a workspace file:
```yaml
jobs:
  - name: "test" # names must be unique within workspace
    repo_url: "git@github.com:n-k/thingy.git"
    build_script: "build.sh" # should be an executable file present in the repository, see build.sh in this repository for example
    poll_interval_seconds: 300 # optional
    auth: # optional
      PrivateKey:
        path: "/path/to/your/ssh/private/key"
        passphrase: "optional - if key has passphrase"
  - name: "test2"
    repo_url: "../../some/path/to/repo.git"
    build_script: "build.sh"
    auth: # optional
      UserPass:
        username: "username"
        password: "password"
```

In this example, it is assumed that the repository contains an executable file `build.sh`. When a new commit is being built, thingy will pull the code, and run `build.sh` in the checkout directory with a few special envronment variables. See next section for list of additional environment variables.

### List of additional environment variables provided to build scripts
- `BRANCH`: name of branch being built
- `COMMIT_HASH`: current commit hash being built
Any environment variables passed to the thingy executable are also passed to the buld processes.

## Features
- Multi-branch Git poll/build
- REST API
- Simple, but functional web interface
- Log viewer, with tailing support for running builds

## Roadmap
- Github account support - allow authenticating with github API token, and listing repositories.
- Support docker builds. It would be good to have more support for docker bulds, but for now, having docker commands in the build scripts works well enough.
- Secrets. It will be good to have better support for secret management. For now, the thingy.yaml file can have git credentials. This file is not expected to be shared or be public, so at least for my setup, it is safe to put credentials in it. 
  For other secrets, any environment variables passed to the thingy executable are passed on to build processes, which can be used to, e.g., provide paths to files containing other secrets.

## FAQ
 1. Why?
 - This has the minimal set of features which I need for my personal projects, and home-lab automation things. Every other alternative seemed overkill for my needs. I also run this on Raspberry Pi's, and this project will always focus on low resource consumption.
 2. Is this going to be maintained? Will you add features?
 - I use this myself, so I will maintain at least the current features and a few more (please see roadmap section). If you would like to see some additional features, please open a Github issue, or send a PR - both are very welcome.
 3. Why only Git support?
 - I only have Git repositories.


# Design
Thingy works on top of Actix actors, and a REST API made with Actix-web. Each component in thingy is an actor.

Actors in thingy form a tree, with one root. The organization looks like this:

 - Thingy actor (root)
    - 1 Actor per job
      - 1 Actor per branch of the job's repository
        - Temporary actors for each build

## Structure of workspace directories
```
workspace_directory/
  thingy.yaml (job definitions)
  job_1/ (directory name is same as job name)
    branch_1/
      data.json (saved state for this branch, contains past/ongoing builds, last seen commit hash)
      build_num.txt (number of latest build to have been started, keeps increasing by 1)
      1/
        repo/ (directory where this build cloned the repository)
          ... files from repo ...
        log.txt (build logs, both stdout and stderr are captured, and prefixed by [out] or [err])
```
