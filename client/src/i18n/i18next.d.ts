import "i18next";
import type { Catalogue } from "./catalogue";

// Types `t()` keys and results from the English catalogue.
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "common";
    resources: Catalogue;
    returnNull: false;
  }
}
