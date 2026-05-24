# FRAG95
Stupid PS1 style 2.5D multiplayer shooter game where I mostly just add whatever I think of

Currently got:
- Multiplayer
- A cool NTSC shader (very similar to the one used in Petscop)
- PS1 effects (vertex snap, affine mapping)
- Bots (they dont do much yet)
- Basic text user interface
- Custom binary level file format
- Level creator HTML page
- Projectiles
- Map loads from a file
- Textures

Soon:
- realistic sound with physics (diffusion, absorption, reflection, etc.)
- Other types of weapons (Hitscan, AOE, mines)
- More advanced bots
- Name tags
- Static objects (like furniture etc)
- Health/damage
- Heads up display
- Multi-map/room server with doors to each room
- Whatever else I feel like adding!

## Technical stuff

I haven't used an engine, but I try to keep the architecture/design as minimal as possible for how simple this game is instead of going overboard.
For example, there is no full ECS, but the net code is fairly detailed as it needs to be.

I used wgsl for graphics and winit for windowing, input, etc.
The rest of the dependencies are pretty standard (glam, bincode, bytemuck, rand, pollster, image, serde)
I reimplemented the NTSC shader in WGSL and made it resolution and framerate independent.

The multiplayer is all UDP with important and unimportant packet types.

This whole project is very unfinisehd and is mostly just a dumb game where I experiment. The goal is just to play it with my friends.
