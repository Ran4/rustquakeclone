A **flatbed truck** sits on the spawn terrace of **level 8**. It's a hand-built low-poly model with
an open cab and a flat, wall-less bed. The deck is a *moving solid surface*: jump up onto the bed,
walk to the open cab, and press **E** on the driver spot to take the wheel —

| Action  | Key |
|---------|-----|
| Throttle | **W** |
| Brake / reverse | **S** |
| Steer left / right | **A** / **D** |
| Leave the truck | **E** |

Steering only bites while you're moving (and reverses when you back up). Hit a wall and the truck
doesn't stop dead — it **bounces**: a head-on crash rebounds you back off the wall, and a glancing
side-swipe **spins** the truck (and slews you toward the way it scrapes along), so you have to fight
the wheel to straighten out after a clip. The harder the hit, the bigger the rebound and spin (a
real clang plays on a solid crash). Tire grip bleeds the sideways slide away over a moment, so the
truck normally tracks where it points but skids and recovers after a bonk. Anything standing on the
bed — you *or a monster* — rides along with it; there are no side-rails, so friction is all that
keeps you aboard. Take a corner too hard, or get spun by a crash, and you can be flung off. You can
still aim and fire while you drive.

Under the hood the truck carries a real velocity vector and an angular velocity rather than a
forward-only scalar, so a wall can throw its momentum any direction. It reserves one slot in the
world collider list and rewrites it every frame from its transform (the same trick the sliding doors
use for a moving brush), so the player's collision, the monsters' pathing, hitscan and line-of-sight
all treat it as solid for free. The swept solver hands back the wall normal it hit; the truck
reflects its velocity off that (restitution) and spins about it for off-centre impacts. A per-frame
"carry" step slides every rider along with the deck's motion and rotation.
