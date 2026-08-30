/// <reference types="vite/client" />

declare module "*.svg?url" {
  const source: string;
  export default source;
}
