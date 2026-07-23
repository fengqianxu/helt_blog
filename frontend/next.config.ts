import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Vinext emits a self-contained Node.js server for the Docker runtime image.
  output: "standalone",
};

export default nextConfig;
