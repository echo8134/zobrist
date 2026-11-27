import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const file = `${screenshots}/game-data/new.pgn`;
const fileText = () => readFileSync(file, "utf8");

const names = ["White", "Black", "Event", "Site", "Date", "Round"] as const;
type Name = (typeof names)[number];

const board = () => browser.$("main cg-board");
const form = () => browser.$("main aside form");
const showData = () => browser.$("aria/Show the game data");
const hideData = () => browser.$("aria/Hide the game data");
const saveButton = () => form().$("aria/Save");
const cancelButton = () => form().$("aria/Cancel");
const field = (name: Name) => form().$(`aria/${name}`);
const choice = () => form().$("select");
const tab = (positionFromLeft: number) =>
  browser.$(
    `header [role="tablist"] > :nth-child(${positionFromLeft}) [role="tab"]`,
  );
const tabNames = () =>
  browser.$$('header [role="tab"]').map((each) => each.getText());

/** The text that the choice shows. The test code has no types of the window. */
async function chosenResult() {
  const text: unknown = await browser.execute(`
    return document.querySelector("main aside form select").selectedOptions[0].textContent;
  `);
  return text;
}

/** The text of every field and of the choice, as the form shows them. */
async function shownData() {
  const texts: Record<string, unknown> = { Result: await chosenResult() };
  for (const name of names) {
    texts[name] = await field(name).getValue();
  }
  return texts;
}

async function typeAndPressEnter(name: Name, text: string) {
  await field(name).setValue(text);
  await browser.keys("Enter");
}

/** A click on an empty square, which moves no piece. */
async function clickOnTheBoard() {
  const squareSide = (await board().getSize("width")) / 8;
  await board().click({ x: 0.5 * squareSide, y: -0.5 * squareSide });
}

async function clickSquare(square: string) {
  const squareSide = (await board().getSize("width")) / 8;
  const fromFileA = "abcdefgh".indexOf(square.charAt(0));
  const fromRank1 = Number(square.charAt(1)) - 1;
  await board().click({
    x: (fromFileA - 3.5) * squareSide,
    y: (3.5 - fromRank1) * squareSide,
  });
}

async function playMove(from: string, to: string) {
  const movesBefore = await browser.$$("main aside li").length;
  await clickSquare(from);
  await clickSquare(to);
  await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(
    movesBefore + 1,
  );
}

/**
 * The choice opens a menu of the system, which the driver of the test cannot reach. So the
 * test sets the value of the choice and tells it of the change, as the menu does.
 */
async function choose(result: string) {
  await browser.execute(`
    const choice = document.querySelector("main aside form select");
    choice.value = [...choice.options].find((option) => option.textContent === "${result}").value;
    choice.dispatchEvent(new Event("change", { bubbles: true }));
  `);
}

const lineColor = async (name: Name) =>
  (await field(name).getCSSProperty("border-top-color")).parsed.hex;
// A field that Rust refused points at the text under it.
const hintOf = async (name: Name) =>
  browser.$(`[id="${await field(name).getAttribute("aria-describedby")}"]`);

describe("the data of a game", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    await browser.$("header").$("aria/New board").click();
    await expect(board()).toExist();
  });

  it("opens the empty game data above the notation", async () => {
    expect(await form().isDisplayed()).toBe(false);
    await expect(showData()).toHaveAttribute("aria-pressed", "false");
    await expect(browser.$("main aside div:has(> ol)")).toBeDisplayed();
    await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(0);
    await expect(browser.$("aria/Comment")).toBeDisplayed();

    await showData().click();

    await expect(form()).toBeDisplayed();
    await expect(hideData()).toHaveAttribute("aria-pressed", "true");
    expect(await saveButton().isDisplayed()).toBe(false);
    expect(await cancelButton().isDisplayed()).toBe(false);
    expect(await shownData()).toEqual({
      White: "",
      Black: "",
      Event: "",
      Site: "",
      Date: "",
      Round: "",
      Result: "*",
    });
    expect(
      await form()
        .$$("label")
        .map((name) => name.getText()),
    ).toEqual(["White", "Black", "Event", "Site", "Date", "Round", "Result"]);

    // The notation area is 320 px wide with a line of 1 px at its left. The form stands
    // under the top bar and the row of buttons, which are 40 px high each.
    expect(await form().getLocation()).toEqual({ x: 1280 - 319, y: 40 + 40 });
    expect(await form().getSize("width")).toBe(319);
    const whiteName = form().$("label=White");
    expect(await whiteName.getLocation()).toEqual({
      x: 961 + 16,
      y: 40 + 40 + 12,
    });
    expect((await whiteName.getCSSProperty("font-size")).value).toBe("11px");
    expect((await whiteName.getCSSProperty("color")).parsed.hex).toBe(
      "#6b6b6b",
    );
    const white = await field("White").getLocation();
    const whiteSize = await field("White").getSize();
    expect(whiteSize).toEqual({ width: (319 - 2 * 16 - 8) / 2, height: 28 });
    const nameBottom =
      (await whiteName.getLocation("y")) + (await whiteName.getSize("height"));
    expect(white.y - nameBottom).toBeCloseTo(2);
    expect((await field("Black").getLocation("x")) - white.x).toBe(
      whiteSize.width + 8,
    );
    const eventName = form().$("label=Event");
    expect((await eventName.getLocation("y")) - (white.y + 28)).toBeCloseTo(8);
    expect(await choice().getSize()).toEqual(whiteSize);
    expect(await lineColor("White")).toBe("#e2e2e2");
    expect(
      (await form().getCSSProperty("border-bottom-color")).parsed.hex,
    ).toBe("#e6e6e6");
    const formBottom =
      (await form().getLocation("y")) + (await form().getSize("height"));
    const resultBottom = (await choice().getLocation("y")) + 28;
    expect(formBottom - resultBottom).toBeCloseTo(12 + 1);
  });

  it("names the tab after both players as soon as both are set", async () => {
    await typeAndPressEnter("White", "Alpha");
    await expect(field("White")).toHaveValue("Alpha");
    expect(await tabNames()).toEqual(["New game"]);

    await field("Black").setValue("Beta");
    await clickOnTheBoard();
    await expect(tab(1)).toHaveText("Alpha - Beta");
    await expect(browser.$("header").$("aria/Close Alpha - Beta")).toExist();

    await hideData().click();
    expect(await form().isDisplayed()).toBe(false);
    await expect(tab(1)).toHaveText("Alpha - Beta");
    await showData().click();
    await expect(field("White")).toHaveValue("Alpha");
    await expect(field("Black")).toHaveValue("Beta");
  });

  it("keeps a date that Rust refuses, marks its field, and says how to write it", async () => {
    await typeAndPressEnter("Date", "26.11.2026");

    await expect(field("Date")).toHaveAttribute("aria-invalid", "true");
    const hint = await hintOf("Date");
    await expect(hint).toHaveText(
      "Write the date as 2026.11.26. Use ?? for a part that is unknown.",
    );
    await expect(field("Date")).toHaveValue("26.11.2026");
    expect(await lineColor("Date")).toBe("#b3261e");
    expect((await hint.getCSSProperty("font-size")).value).toBe("11px");
    expect((await hint.getCSSProperty("color")).parsed.hex).toBe("#b3261e");
    const date = await field("Date").getLocation();
    expect(await hint.getLocation("x")).toBe(date.x);
    expect((await hint.getLocation("y")) - (date.y + 28)).toBeCloseTo(2);

    await typeAndPressEnter("Date", "2026.11.26");

    await expect(field("Date")).toHaveAttribute("aria-invalid", "false");
    await expect(form().$("span")).not.toExist();
    await expect(field("Date")).toHaveValue("2026.11.26");
    expect(await lineColor("Date")).toBe("#e2e2e2");
  });

  it("takes the event, the site, and the round, and a text with white space at its ends", async () => {
    await typeAndPressEnter("Event", "  Club match ");
    await expect(field("Event")).toHaveValue("Club match");
    await typeAndPressEnter("Site", "Amsterdam");
    await typeAndPressEnter("Round", "2");
    await clickOnTheBoard();

    expect(await shownData()).toEqual({
      White: "Alpha",
      Black: "Beta",
      Event: "Club match",
      Site: "Amsterdam",
      Date: "2026.11.26",
      Round: "2",
      Result: "*",
    });
  });

  it("sets the result of the game with the choice, and an undo takes it back", async () => {
    await choose("1-0");
    await browser.waitUntil(async () => (await chosenResult()) === "1-0");

    await playMove("e2", "e4");
    await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(1);
    expect(await chosenResult()).toBe("1-0");

    await clickOnTheBoard();
    await browser.keys(["Control", "z"]);
    await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(0);
    expect(await chosenResult()).toBe("1-0");
    await browser.keys(["Control", "z"]);
    await browser.waitUntil(async () => (await chosenResult()) === "*");

    await browser.keys(["Control", "y"]);
    await browser.waitUntil(async () => (await chosenResult()) === "1-0");
    await browser.keys(["Control", "y"]);
    await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(1);
  });

  it("keeps a key from the game while a field of the form has the focus", async () => {
    const currentMoves = () => browser.$$("main aside [aria-current]");
    await field("Site").click();
    await browser.keys("ArrowLeft");
    // Nothing is to happen, so there is nothing to wait for but time.
    await browser.pause(300);
    await expect(currentMoves()).toBeElementsArrayOfSize(1);

    await clickOnTheBoard();
    await browser.keys("ArrowLeft");
    await expect(currentMoves()).toBeElementsArrayOfSize(0);
    await browser.keys("ArrowRight");
    await expect(currentMoves()).toBeElementsArrayOfSize(1);
  });

  it("keeps what the player types on while Rust answers the text before", async () => {
    // The driver of the test is too slow to type between a text and its answer. So the
    // script sends a text with Enter and types on in the same moment, before Rust can answer.
    await browser.execute(`
      const white = document.querySelector("main aside form input");
      const type = (text) => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(white, text);
        white.dispatchEvent(new InputEvent("input", { bubbles: true }));
      };
      white.focus();
      type("Gamma");
      white.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      type("Gammas");
    `);

    // The name of the tab shows that the answer for the first text came.
    await expect(tab(1)).toHaveText("Gamma - Beta");
    await expect(field("White")).toHaveValue("Gammas");

    await clickOnTheBoard();
    await expect(tab(1)).toHaveText("Gammas - Beta");

    await typeAndPressEnter("White", "Alpha");
    await expect(tab(1)).toHaveText("Alpha - Beta");
  });

  it("gives each tab a form of its own", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(2)).toHaveAttribute("aria-selected", "true");

    expect(await tabNames()).toEqual(["Alpha - Beta", "New game"]);
    expect(await form().isDisplayed()).toBe(false);
    await showData().click();
    expect(await shownData()).toEqual({
      White: "",
      Black: "",
      Event: "",
      Site: "",
      Date: "",
      Round: "",
      Result: "*",
    });

    await tab(1).click();
    await expect(field("White")).toHaveValue("Alpha");
    expect(await shownData()).toEqual({
      White: "Alpha",
      Black: "Beta",
      Event: "Club match",
      Site: "Amsterdam",
      Date: "2026.11.26",
      Round: "2",
      Result: "1-0",
    });
  });

  it("asks for the game data before the first save and writes the focused field", async () => {
    mkdirSync(`${screenshots}/game-data`, { recursive: true });
    rmSync(file, { force: true });
    await browser.$("nav").$("aria/Files").click();
    await browser.$("aria/Path of a PGN file").setValue(file);
    await browser.$("aria/Create").click();
    await expect(browser.$('main [role="status"]')).toHaveText(
      "The file has no games.",
    );
    await browser.$("header").$("aria/New board").click();
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
    await playMove("e2", "e4");
    await expect(browser.$("main aside [aria-current]")).toHaveText("e4");
    await browser.$("aria/Save").click();

    await expect(saveButton()).toBeDisplayed();
    await expect(cancelButton()).toBeDisplayed();
    await expect(hideData()).toHaveAttribute("aria-pressed", "true");
    expect(fileText()).toBe("");
    await field("White").setValue("Gamma");
    expect(await saveButton().getSize("height")).toBe(28);
    expect(await cancelButton().getSize("height")).toBe(28);
    const save = await saveButton().getLocation();
    expect(save.x).toBe(await choice().getLocation("x"));
    expect(save.y - (await choice().getLocation("y")) - 28).toBeCloseTo(12);
    expect(
      (await cancelButton().getLocation("x")) -
        save.x -
        (await saveButton().getSize("width")),
    ).toBeCloseTo(8);
    await expect(board().$("piece.anim")).not.toExist();
    const picture = await browser.saveScreenshot(
      `${screenshots}/game-data.png`,
    );
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });

    await saveButton().click();

    await expect(tab(3).$("span:empty")).not.toExist();
    expect(await form().isDisplayed()).toBe(false);
    expect(fileText()).toBe('[White "Gamma"]\n\n1. e4 *\n');
  });

  it("keeps a cancelled game unsaved and accepts Enter outside a field", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(4)).toHaveAttribute("aria-selected", "true");
    await playMove("d2", "d4");
    await expect(browser.$("main aside [aria-current]")).toHaveText("d4");
    const firstGame = '[White "Gamma"]\n\n1. e4 *\n';

    await browser.$("aria/Save as new game").click();
    await expect(saveButton()).toBeDisplayed();
    await hideData().click();
    expect(await form().isDisplayed()).toBe(false);
    expect(fileText()).toBe(firstGame);

    await browser.keys(["Control", "s"]);
    await field("White").click();
    await browser.keys("Escape");
    expect(await form().isDisplayed()).toBe(false);
    expect(fileText()).toBe(firstGame);

    await browser.keys(["Control", "s"]);
    await expect(saveButton()).toBeDisplayed();
    await expect(cancelButton()).toBeDisplayed();
    expect(fileText()).toBe(firstGame);
    await cancelButton().click();

    expect(await form().isDisplayed()).toBe(false);
    await expect(tab(4).$("span:empty")).toExist();
    expect(fileText()).toBe(firstGame);

    await browser.keys(["Control", "s"]);
    await expect(saveButton()).toBeDisplayed();
    await browser.keys("Enter");

    await expect(tab(4).$("span:empty")).not.toExist();
    expect(await form().isDisplayed()).toBe(false);
    expect(fileText()).toBe(`${firstGame}\n1. d4 *\n`);
  });

  it("saves the next move in place as soon as the player clicks Save", async () => {
    await tab(3).click();
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
    await playMove("e7", "e5");
    await expect(browser.$("main aside [aria-current]")).toHaveText("e5");
    await browser.$("aria/Save").click();

    await expect(tab(3).$("span:empty")).not.toExist();
    expect(await form().isDisplayed()).toBe(false);
    expect(fileText()).toBe('[White "Gamma"]\n\n1. e4 e5 *\n\n1. d4 *\n');
  });

  it("cancels the first save when Enter is pressed on Cancel", async () => {
    const before = fileText();
    await browser.$("header").$("aria/New board").click();
    await expect(tab(5)).toHaveAttribute("aria-selected", "true");
    await playMove("c2", "c4");
    await browser.keys(["Control", "s"]);
    await expect(cancelButton()).toBeDisplayed();
    await browser.execute("arguments[0].focus();", await cancelButton());
    await expect(cancelButton()).toBeFocused();

    // A synthetic Enter does not activate a button. Apply its default click only if
    // the key reaches the focused button and the app leaves it uncancelled.
    await browser.execute(`
      const button = document.activeElement;
      button.addEventListener("keydown", (event) => {
        queueMicrotask(() => {
          if (event.key === "Enter" && !event.defaultPrevented) button.click();
        });
      }, { once: true });
    `);
    await browser.keys("Enter");

    await expect(showData()).toBeDisplayed();
    expect(await form().isDisplayed()).toBe(false);
    expect(fileText()).toBe(before);
    await expect(tab(5).$("span:empty")).toExist();
  });
});
