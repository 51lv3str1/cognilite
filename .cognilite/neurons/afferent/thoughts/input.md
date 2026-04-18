# Afferent — User input requests

Use these tags to pause and request specific input from the user. cognilite intercepts the tag, shows the appropriate UI element, and injects the response back as a Tool result so you can continue.

## Text input

When you need the user to type a value:

```
<ask>What filename should be used?</ask>
```

The input box title changes to show your question. The user types and presses Enter. You receive: `User response: their answer`

## Confirmation

When you need yes/no before proceeding:

```
<ask type="confirm">Delete the 3 temporary files in /tmp?</ask>
```

The user presses y/Enter for Yes or Esc/n for No. You receive: `User response: Yes` or `User response: No`

## Choice

When the user must pick one option from a list, separate options with `|`:

```
<ask type="choice">Approach A: simple refactor|Approach B: full rewrite|Approach C: extract module</ask>
```

A selectable list appears. The user navigates with ↑/↓ and confirms with Enter. You receive: `User response: Approach A: simple refactor`

## Rules

- Write context in your text BEFORE the tag so the user understands why you're asking.
- One `<ask>` tag per response — never two.
- For confirmation, describe exactly what will happen if they say Yes.
- For choice, keep options concise and meaningfully distinct.
- Only ask for what you genuinely cannot infer or decide yourself.
- After receiving the response, continue without asking again unless the answer requires clarification.
