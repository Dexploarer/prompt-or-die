Original prompt: its the graphics and the worlds scene, everything is piled up on eacho other the hid is trash this is supposed to be AAA quality

- Investigating the `pod-web` flagship sandbox for real visual issues instead of only checking systems health.
- Confirmed the page is live, but the current camera rig and default HUD presentation make the world look broken on first impression.
- Immediate fix plan:
  - stop camera auto-zoom from fitting the whole active shard
  - use a more grounded third-person camera rig
  - spread the authored sandbox hub and landmark props
  - demote debug heatmaps/details out of the primary gameplay HUD
  - remove the accidental render-space compression that collapsed world coordinates by 0.08x
