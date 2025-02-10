/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  output: 'standalone',
  experimental: {
    serverActions: true,
  },
  distDir: '.next',
  poweredByHeader: false,
}

module.exports = nextConfig
