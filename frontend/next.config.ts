import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Vinext emits a self-contained Node.js server for the Docker runtime image.
  output: "standalone",
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "**.hdslb.com" },
      { protocol: "https", hostname: "shared.cloudflare.steamstatic.com" },
      { protocol: "https", hostname: "media.steampowered.com" },
    ],
  },
};

export default nextConfig;
