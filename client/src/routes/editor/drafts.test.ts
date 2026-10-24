import { describe, expect, test } from "bun:test";
import { openStore } from "../../lib/store";
import { newForm } from "./convert";
import {
  deleteDraft,
  describeDraft,
  draftIdFor,
  hrefForDraft,
  isFormState,
  loadDraft,
  NEW_DRAFT_ID,
  saveDraft,
} from "./drafts";

let counter = 0;
async function drafts() {
  counter += 1;
  return (await openStore(`editor-drafts-${counter}`)).drafts;
}

function form() {
  return {
    ...newForm({
      lang: "en",
      identity: {
        author_fpr: new Uint8Array(),
        title: "",
        language: "en",
        created_at: 0,
        birth_nonce: new Uint8Array([1, 2, 3]),
      },
    }),
    title: "Soup",
  };
}

describe("draft ids", () => {
  test("new and edit ids round-trip to routes", () => {
    expect(draftIdFor()).toBe(NEW_DRAFT_ID);
    expect(draftIdFor("rcp1abc")).toBe("edit:rcp1abc");
    expect(hrefForDraft(draftIdFor())).toBe("/new");
    expect(hrefForDraft(draftIdFor("rcp1abc"))).toBe("/edit/rcp1abc");
  });
});

describe("persistence", () => {
  test("save, load, and delete", async () => {
    const repo = await drafts();
    expect(await loadDraft("new", repo)).toBeNull();
    await saveDraft("new", form(), repo);
    const loaded = await loadDraft("new", repo);
    expect(loaded?.title).toBe("Soup");
    expect(loaded?.identity.birth_nonce).toEqual(new Uint8Array([1, 2, 3]));
    await deleteDraft("new", repo);
    expect(await loadDraft("new", repo)).toBeNull();
  });

  test("unreadable stored state loads as nothing", async () => {
    const repo = await drafts();
    await repo.put({ id: "new", state: { form: { v: 99 } }, updatedAt: 1 });
    expect(await loadDraft("new", repo)).toBeNull();
  });
});

describe("isFormState", () => {
  test("accepts editor state and rejects other shapes", () => {
    expect(isFormState(form())).toBe(true);
    expect(isFormState(null)).toBe(false);
    expect(isFormState({ ...form(), ingredients: [{ ref: "1" }] })).toBe(false);
    expect(isFormState({ ...form(), nextRef: 1.5 })).toBe(false);
  });
});

describe("describeDraft", () => {
  test("summarizes drafts for the Library", () => {
    expect(
      describeDraft({
        id: "edit:rcp1x",
        state: { form: form() },
        updatedAt: 5,
      }),
    ).toEqual({
      id: "edit:rcp1x",
      title: "Soup",
      editing: true,
      href: "/edit/rcp1x",
      updatedAt: 5,
    });
    expect(
      describeDraft({ id: "new", state: "junk", updatedAt: 1 }),
    ).toBeNull();
  });

  test("reads the title of a draft as saveDraft stores it", async () => {
    const repo = await drafts();
    await saveDraft("new", { ...form(), title: " Stew " }, repo);
    const record = await repo.get("new");
    if (!record) throw new Error("the draft was not stored");
    expect(describeDraft(record)).toMatchObject({
      title: "Stew",
      editing: false,
      href: "/new",
    });
  });
});
