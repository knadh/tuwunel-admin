# tuwunel-admin

A simple, self-contained web admin UI for [tuwunel](https://github.com/matrix-construct/tuwunel) Matrix chat server. Tuwunel has no HTTP admin API and its administration is done by sending text commands to the server's admin room (conduwuit convention). This is unfortunately an extremely clunky, difficult UX.

tuwunel-admin logs in as the Matrix user, and provides a simple web-based UI/UX abstracting the `!admin` message commands and functionality underneath it.

- Create, list, and manage rooms and federation.
- Create, list, and manage users.
- View and manage media.
- View and registration tokens.
- View server configuration, run maintenance commands.
- etc ..

> [!NOTE]  
> I've cobbled this together for my personal use and have only used it to administer a single non-federated tuwunel instance. The project is sub-v1.0.0 and may contain bugs. Use with care.

## Usage

Download the latest binary [release](https://github.com/knadh/tuwunel-admin/releases).

- Generate a new config file by running `./tuwunel-admin new-config` and edit the config file.
- Run the server with `./tuwunel-admin` and visit `http://localhost:8009`


## Screenshots
#### Login
<img width="511" height="501" alt="Image" src="https://github.com/user-attachments/assets/9ea5ea53-d791-4975-ac97-5c1d00037810" />

#### Dashboard
<img width="7204" height="3200" alt="Image" src="https://github.com/user-attachments/assets/43a812e5-4d7e-421a-ab17-8430520fcb64" />

#### Users

<img width="7204" height="3200" alt="Image" src="https://github.com/user-attachments/assets/7a8cb520-03fb-42cd-b466-61ecd9854646" />

#### Rooms

<img width="7204" height="3200" alt="Image" src="https://github.com/user-attachments/assets/0eb62b56-013b-486a-bdc4-7db9f8aaf1a3" />

-------

Licensed under the Apache 2 license.
