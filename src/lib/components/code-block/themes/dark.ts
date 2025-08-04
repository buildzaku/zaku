import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";

export const dark = EditorView.theme(
  {
    "&": {
      color: "#d4d7e8",
      backgroundColor: "transparent",
    },
    ".cm-content": {
      caretColor: "#c6c6c6",
      fontFamily: "monospace",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "#c6c6c6",
    },
    "&.cm-focused .cm-selectionBackground, ::selection": {
      backgroundColor: "#6199ff2f",
    },
    ".cm-selectionMatch": {
      backgroundColor: "#72a1ff59",
    },
    ".cm-line": {
      backgroundColor: "transparent",
    },
    ".cm-activeLine": {
      boxShadow: "inset 0px 0px 400px 110px rgba(0, 0, 0, 0.25)",
    },
    ".cm-gutters": {
      backgroundColor: "#1e1e1e",
      color: "#838383",
      border: "none",
      cursor: "default",
    },
    ".cm-activeLineGutter": {
      color: "#fff",
      backgroundColor: "transparent",
    },
  },
  { dark: true },
);

const highlights = HighlightStyle.define([
  {
    tag: [
      tags.keyword,
      tags.operatorKeyword,
      tags.modifier,
      tags.color,
      tags.constant(tags.name),
      tags.standard(tags.name),
      tags.standard(tags.tagName),
      tags.special(tags.brace),
      tags.atom,
      tags.bool,
      tags.special(tags.variableName),
    ],
    color: "#569cd6",
  },
  { tag: [tags.controlKeyword, tags.moduleKeyword], color: "#c586c0" },
  {
    tag: [
      tags.name,
      tags.deleted,
      tags.character,
      tags.macroName,
      tags.propertyName,
      tags.variableName,
      tags.labelName,
      tags.definition(tags.name),
    ],
    color: "#62b3dd",
  },
  { tag: tags.heading, fontWeight: "bold", color: "#62b3dd" },
  {
    tag: [
      tags.typeName,
      tags.className,
      tags.tagName,
      tags.number,
      tags.changed,
      tags.annotation,
      tags.self,
      tags.namespace,
    ],
    color: "#4ec9b0",
  },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: "#dcdcaa" },
  { tag: [tags.number], color: "#b5cea8" },
  {
    tag: [tags.operator, tags.punctuation, tags.separator, tags.url, tags.escape, tags.regexp],
    color: "#d4d4d4",
  },
  { tag: [tags.regexp], color: "#d16969" },
  {
    tag: [tags.special(tags.string), tags.processingInstruction, tags.string, tags.inserted],
    color: "#ce9178",
  },
  { tag: [tags.angleBracket], color: "#808080" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: [tags.meta, tags.comment], color: "#6a9955" },
  { tag: tags.link, color: "#6a9955", textDecoration: "underline" },
  { tag: tags.invalid, color: "#ff0000" },
]);

export const darkTheme = [dark, syntaxHighlighting(highlights)];
