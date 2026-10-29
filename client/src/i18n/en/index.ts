import { browse } from "./browse";
import { common } from "./common";
import { cook } from "./cook";
import { editor } from "./editor";
import { editorLists } from "./editorLists";
import { library } from "./library";
import { recipe } from "./recipe";
import { settings } from "./settings";

/**
 * The English catalogue: the fallback every other locale is checked
 * against, and the source of the key types `t()` accepts.
 */
export const en = {
  common,
  library,
  recipe,
  cook,
  editor,
  editorLists,
  settings,
  browse,
} as const;
