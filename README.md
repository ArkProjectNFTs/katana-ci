# Katana-CI

This repository contains a tool that helps starting Katana sequencer on demand (mostly for CI purposes).
To work, the `katana-ci` binary must be deployed on a server with docker installed.

## Design

To keep things simple and flexible for CI, the `katana-ci` uses docker under the hood.
With an `file` database for now, each user have an `api-key` that allows the `start` and `stop` of a Katana instance.
When a user starts an instance, a new container is created and started. 

The database trait `ProxifierDb` is for now targetting `Sqlite`, but may be reworked to support any backend supported by `sqlx` rust crate.

## Server configuration

To quickly spin up `katana-ci` server, you can do the following:

1. Install docker on your machine, for example: `snap install docker`.
2. Download the binary from the [release](https://github.com/ArkProjectNFTs/katana-ci/releases/tag/v0.1.0) (or build from source)
   ```bash
   wget -q -O katana-ci https://github.com/ArkProjectNFTs/katana-ci/releases/download/v0.1.0/katana-ci_amd64
   chmod +x katana-ci
   ```
3. Pull the docker image of katana (or use your own):
   ```bash
   docker pull arkproject/katana:0.3.1
   ```
4. Setup environment file with initial users and docker image ID:
   ```bash
   # .env
   export KATANA_CI_IMAGE=<ID_OF_IMAGE>
   export KATANA_CI_USERS_FILE=.users
   ```
   The `.users` file contains a simple list of initial users with their name and API-KEY, for example:
   ```bash
   user1,mykey
   user2,1234
   ```
5. Run the binary to be ready to spawn katana instances from your CI:
   ```bash
   # Create the database file for SQLite. In the current version, if you
   # don't delete the file and restart the binary you will see an insertion error
   # of the users, that's normal, users are still in the DB and we don't check for existence.
   touch data.db
   # Source environment variables.
   source .env
   # Let's run.
   ./katana-ci
   ```

## GitHub CI

To setup your action in the GitHub CI, you can check the full example in [.github/workflows/example.yml](https://github.com/ArkProjectNFTs/katana-ci/blob/main/.github/workflows/example.yml), and you also have a test in [examples/e2e.rs](https://github.com/ArkProjectNFTs/katana-ci/blob/main/examples/e2e.rs) with
all basic actions on a contract with `starknet-rs` -> declare, deploy (which is an invoke with the UDC), call.

You can use the [katana-ci-action](https://github.com/ArkProjectNFTs/katana-ci-action) to abstract the call to the `katana-ci` server.

Basically, you call the action first to start the katana:
```yml
    - name: Startup Katana CI instance
      id: katanaci
      uses: ArkProjectNFTs/katana-ci-action@v1
      with:
        api-url: ${{ secrets.KATANA_CI_URL }}
        api-key: ${{ secrets.KATANA_CI_KEY }}
        cmd: 'start'
```
You can then inject the URL of the spawned instance in your tests, in the example we use environment variable to pass it:
```yml
    - name: Run cargo test
      uses: actions-rs/cargo@v1
      env:
        STARKNET_RPC: ${{ steps.katanaci.outputs.katana-rpc }}
      with:
        command: run
        args: --example e2e
```
And then you call it again to stop it:
```yml
    - name: Terminate Katana CI instance
      uses: ArkProjectNFTs/katana-ci-action@v1
      with:
        api-url: ${{ secrets.KATANA_CI_URL }}
        api-key: ${{ secrets.KATANA_CI_KEY }}
        cmd: 'stop'
        name: ${{ steps.katanaci.outputs.katana-name }}
```
It can be a good refacto to use `js` actions, in order to leverage the `post` cleanup.

## Usage from the CLI

The idea is to have the least overhead possible to spawn and interact with a Katana instance. So no client is designed for now,
and you can use very common tools like `curl`.

1. Start an instance
   ```bash
   curl -H 'Authorization: Bearer mykey' https://<your_backend_url>/start

   # Returns a simple string with the name of the created instance.
   4f2b3c60ae32
   ```
   The start will return an instance `name`, that can then be used to target Katana for this specific instance.
   The `name` returned is always URL friendly.

2. Use `starkli` to interact with the instance, for example:
   ```bash
   starkli block --full --rpc https://<your_backend_url>/<name>/katana
   ```

3. To check the logs, you can hit the endpoint `/logs` of your instance, by default it returns `25` tail lines. You can use `all` or any number you like using the query parameter `n`.
   ```bash
   curl -H 'Authorization: Bearer mykey' https://<your_backend_url>/<name>/logs
        
   curl -H 'Authorization: Bearer mykey' https://<your_backend_url>/<name>/logs?n=100
   ```

4. Then, you can stop the instance if it's no longer needed.
   ```bash
   curl -H 'Authorization: Bearer mykey' https://<your_backend_url>/<name>/stop
   ```
