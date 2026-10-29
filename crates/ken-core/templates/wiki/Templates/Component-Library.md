---
title: "Component Library"
aliases: ["Component Library", "design system", "is there already a component for this", "is this component locked", "how do I add a component"]
status: current
audience: dev
updated: {{date}}
sources:
  - "{{repo@sha:path of the token contract}}"
  - "{{repo@sha:path of the lock checklist}}"
  - "{{repo@sha:path of the conformance test}}"
---

# Component Library

Need a component for a screen? Check here whether a locked one already exists, and if it doesn't, how to add one. Here are the library's packages, its theming layers, the rules every component follows and the checklist that locks one. Not here: why a screen looks the way it does → its note on [[Design]]; the team's other rules → [[Rules]].

[[#Packages]] · [[#Theming]] · [[#Rules]] · [[#Workbench]] · [[#Lock Checklist]] · [[#Adding a Component]] · [[#Where Things Live]]

## Packages

Dependencies run one way. The app uses {{domain package}}, {{domain package}} uses {{primitives package}}, and nothing points back.

| package | holds | uses | never uses |
|---|---|---|---|
| {{primitives package}} | {{the base components from the kit · the theming system · the catalog under `{{root}}.*` · the link and image components · the test id helpers}} | {{the kit · the styling tool · the icon set}} | {{the domain package · the model · the app's framework}} |
| {{domain package}} | {{components with the product's names · the catalog under `{{root}}.*`}} | {{the primitives · the model}} | {{the app's framework · the app's data layer}} |
| {{model package}} | {{the product's data as schemas, with inferred types}} | {{the schema library}} | anything UI |
| {{workbench}} | every story · the foundations · the lock checklist · the demos | both packages, read from source | package internals |
| {{reference app}} | {{what it shows}} | both packages · {{the framework's link and image}} | |

{{The naming rule: the primitives keep the kit's names, domain components use the product's words and never reuse a primitive's name, and whether either layer uses a prefix.}}

## Theming

| layer | set with | controls | ships with |
|---|---|---|---|
| theme | `data-theme` | structure: fonts · radius · control heights · type sizes · surfaces | {{the themes}} |
| colourway | `data-colorway` | every colour token, light and dark | {{the colourways}} |
| mode | {{the `dark` class}} | which half of the colourway applies | light · dark |

The token contract is `{{path}}`. {{How a brand colourway is made from two colours, and how an app adds a theme or colourway of its own.}}

## Rules

The method's eight rules are on the Design page in the manual, each with its reason. Mark each one kept or dropped. A dropped rule says why, so a reviewer doesn't flag what the team decided against.

| rule | kept | how it's done here | why it was dropped or added |
|---|---|---|---|
| test ids | {{yes}} | {{`data-testid="<prefix>-<slot>"`, before the caller's props}} | |
| props types | {{yes}} | {{`<Name>Props`, exported}} | |
| part props | {{yes}} | {{`<slot>Props` for parts the component renders itself}} | |
| no built-in strings | {{yes}} | {{the catalog, or required props on server-safe components}} | |
| catalog roots | {{yes}} | {{the roots: `{{root}}.*`}} | |
| links and images | {{yes}} | {{the link and image components}} | |
| server-safe by default | {{yes}} | {{when a component runs on the client}} | |
| tokens only | {{yes}} | {{the token contract}} | |
| {{a rule this library adds}} | added | {{the rule}} | {{why it was added}} |

The conformance test is `{{conformance command}}`, in `{{path}}`. G2 runs it with the fast suite.

## Workbench

Run it with `{{command}}`. {{The sidebar sections, the toolbar switches and the checking panels.}}

## Lock Checklist

A component is locked when it meets every criterion in `{{path of the checklist}}`. The domain package builds only on locked primitives. A change to a component updates its row in the same PR, and G4 checks it at close-out.

| criterion | what it requires |
|---|---|
| test ids | the prefixed slot id on every part, replaceable · `<slot>Props` for parts the component renders itself |
| theme | size, spacing, type and radius from structure tokens |
| colourway | every colour from colourway tokens, light and dark |
| strings | no built-in strings |
| typed | an exported `<Name>Props`, with no `any` |
| model | model data in props checked against the schemas in development, for components that take model data |
| server-safe | client-only where state, effects, context, refs or handlers need it |
| link and image | links and images through the library's components, for components that render them |
| stories | a story for every variant, size and state |
| keyboard | a play test for any keyboard behaviour the component adds |
| accessibility | no WCAG 2.2 AA violations in any colourway, light or dark |
| {{a criterion this library adds}} | {{what it requires}} |

## Adding a Component

1. {{Where a primitive comes from, and where a domain component's folder goes.}}
2. Apply the rules above.
3. Move the strings to the catalog, under the package's root.
4. {{Domain package only: take model types.}}
5. Write the stories in the workbench, importing the package entry.
6. Update the checklist row.
7. Run `{{tests}}`, the typecheck and lint, and check the accessibility panel across colourways. Then record the demo with `{{demo command}}`.

## Where Things Live

| path | what is there |
|---|---|
| `{{path}}` | {{the primitives}} |
| `{{path}}` | {{the token contract · the themes · the colourways}} |
| `{{path}}` | {{the catalogs}} |
| `{{path}}` | {{the shared-logic tests and the conformance test}} |
| `{{path}}` | {{the domain components, one folder each}} |
| `{{path}}` | {{the stories}} |
| `{{path}}` | {{the lock checklist}} |
| `{{path}}` | {{the recorded demos}} |
