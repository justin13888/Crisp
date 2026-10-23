/// Build-time constants injected by rsbuild (`import.meta.env`).

interface ImportMetaEnv {
  /** `true` in production builds. */
  readonly PROD: boolean;
  /** `true` in development builds. */
  readonly DEV: boolean;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
