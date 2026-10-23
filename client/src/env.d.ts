/// Ambient declarations for non-code assets.

declare module "*.css";

declare module "*.svg" {
  const url: string;
  export default url;
}
