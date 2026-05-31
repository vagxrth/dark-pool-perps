import { fileURLToPath } from "url";
import { dirname } from "path";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // This app lives inside the Anchor/Arcium workspace (which has its own
  // pnpm-lock.yaml); pin the Turbopack root to this folder so Next doesn't
  // infer the parent as the workspace root.
  turbopack: {
    root: __dirname,
  },
};

export default nextConfig;
