# thingy
Lightweight build server and thing-doer

## Using thingy
Thingy is a command line application and works inside a 'workspace' folder. A thingy workspace is a plain folder with a `thingy.yaml` file in it. This file's structure is based on [this struct](./src/models.rs#L4). This file lists build jobs and configurations.

An example of a workspace file:
```yaml
jobs:
  - name: "test"
    repo_url: "git@github.com:n-k/thingy.git"
    branch: "master"
    build_script: "build.sh" # should be an executable file present in the repository
    poll_interval_seconds: 300
    auth: # optional
      PrivateKey: # currently only supported method, besides no auth
        path: "/path/to/your/ssh/private/key"
        passphrase: "optional - if key has passphrase"

```
In this example, it is assumed that the repository contains an executable file `build.sh`. When a new commit is being built, thingy will pull the code, set CWD to the checkout directory, and run `build.sh` with a few special envronment variables. See next section for list of additional environment variables.

### List of environment variables provided to build scripts
- `BRANCH`: name of branch being built
- `COMMIT_HASH`: current commit hash being built

## Roadmap
- [x] Single branch Git poll/build
- [ ] Multi-branch Git poll/build
- [ ] Web hooks
- [ ] Secrets (other than auth)

## FAQ
 1. Why?
 - This has the minimal set of features which I need for my personal projects, and home-lab automation things. Every other alternative seemed overkill for my needs. I also run this on Raspberry Pi's, and this project will always focus on low resource consumption.
 2. Is this going to be maintained? Will you add features?
 - I use this myself, so I will maintain at least the current features and a few more (please see roadmap section). If you would like to see some additional features, please open a Github issue, or send a PR.
 3. Why only Git support?
 - I only have Git repositories. PRs are very welcome for supporting others.
