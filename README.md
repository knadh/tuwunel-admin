# tuwunel-admin

A simple, self-contained web admin UI for [tuwunel](https://github.com/matrix-construct/tuwunel) Matrix chat server.

Tuwunel has no HTTP admin API and its administration is done by sending text commands to the server's admin room (conduwuit convention). This extremely clunky, difficult to use UX.

This program logs in as the Matrix user, and provides HTML UI/UX and functionality abstracting the `!admin` message commands underneath it.

- Create, list, and manage rooms and federation.
- Create, list, and manage users.
- View and manage media.
- View and registration tokens.
- View server configuration, run maintenance commands.
- etc ..

## Usage

Download the latest binary [release](https://github.com/knadh/tuwunel-admin/releases).

- Generate a new config file by running `./tuwunel-admin new-config` and edit the config file.
- Run the server with `./tuwunel-admin` and visit `http://localhost:8009`

Licensed under the Apache 2 license.
