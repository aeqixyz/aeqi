import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      {
        source: "/js/script.js",
        destination: "http://127.0.0.1:8000/js/script.js",
      },
      {
        source: "/api/event",
        destination: "http://127.0.0.1:8000/api/event",
      },
    ];
  },
};

export default nextConfig;
