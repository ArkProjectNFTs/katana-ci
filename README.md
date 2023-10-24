# Katana-CI

This repository contains a tool that helps starting Katana sequencer on demand (mostly for CI purposes).
To work, the `katana-ci` binary must be deployed on a server with docker installed.

## Design

To keep things simple and flexible for CI, the `katana-ci` uses docker under the hood.
With an `file` database for now, each user have an `api-key` that allows the `start` and `stop` of a Katana instance.
When a user starts an instance, a new container is created and started. 

The database trait `ProxifierDb` is for now targetting `Sqlite`, but may be reworked to support any backend supported by `sqlx` rust crate.

## Usage

The idea is to have the least overhead possible to spawn and interact with a Katana instance. So no client is designed for now,
and you can use very common tools like `curl`.

1. Start an instance
   ```bash
   curl -H 'Content-Type: application/json' \
        -H 'Authorization: Bearer my-key' \
        https://<your_backend_url>/start

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
   curl -H 'Content-Type: application/json' \
        -H 'Authorization: Bearer my-key' \
        https://<your_backend_url>/<name>/logs
        
   curl -H 'Content-Type: application/json' \
        -H 'Authorization: Bearer my-key' \
        https://<your_backend_url>/<name>/logs?n=100
   ```

4. Then, you can stop the instance if it's no longer needed.
   ```bash
   curl -H 'Content-Type: application/json' \
        -H 'Authorization: Bearer my-key' \
        https://<your_backend_url>/<name>/stop
   ```

## GitHub CI

Work in progress.
