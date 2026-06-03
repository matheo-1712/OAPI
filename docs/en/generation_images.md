# Image Generation Service

The image generation service (`src/services/image_service.rs`) dynamically creates high-fidelity visuals for Discord and Minecraft profiles.

---

## Caching Logic

The service uses an intelligent cache to minimize CPU and bandwidth usage.

1.  **Organization**: Images are stored in `public/generated_images/` in specific subfolders (`discord_summary/` or `minecraft_summary/`) grouped by unique identifier (`discord_id` or `account_id`).
2.  **Content Hash**: A SHA-256 hash is generated from **all profile data** (username, statistics, badges, servers).
3.  **Verification**: If the `{hash}.png` file exists, it is served immediately.
4.  **Self-Cleaning**: When a new image is generated (due to data changes), the user's folder is cleared to keep only the most recent version.

---

## Profile Features

### 1. Discord Summary
- **Avatar**: Anti-aliased circle with a border.
- **Stats**: Messages sent and Voice time.
- **Highlights**: Displays the "Best Otter Friend" (the player you've spent the most time with in voice chat) and your favorite roles.

### 2. Minecraft Summary
- **Avatar**: 3D player head (retrieved via UUID).
- **Stats**: Playtime, Distance (in blocks), Blocks mined/placed, Deaths, and Kills.
- **Highlights**: Displays the **top 3 favorite servers** as colored pills matching the server's color in the database.

---

## Technical Drawing Details

- **Engine**: Uses the `image` (pixel manipulation) and `rusttype` (typography) crates.
- **Formatting**: Large numbers are formatted with thousands separators (e.g., `1 250 000`) for optimal readability.
- **Pills (Badges)**: Server or role labels support automatic line wrapping to prevent overflow.
- **Colors**: Full support for HEX codes for borders and accents.

---

## Public URLs
The paths returned by the API are ready to be used in an `<img>` tag:
`/generated_images/minecraft_summary/{uuid}/{hash}.png`
