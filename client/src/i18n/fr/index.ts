import type { Namespace, Translation } from "../catalogue";
import { browse } from "./browse";
import { common } from "./common";
import { cook } from "./cook";
import { editor } from "./editor";
import { editorLists } from "./editorLists";
import { library } from "./library";
import { recipe } from "./recipe";
import { settings } from "./settings";

/** The French catalogue; the type check holds it to every English key. */
export const fr = {
  common,
  library,
  recipe,
  cook,
  editor,
  editorLists,
  settings,
  browse,
} satisfies { [N in Namespace]: Translation<N> };
