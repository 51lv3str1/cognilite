Auditá este proyecto desde la perspectiva de seguridad y riesgos operacionales.

<load_neuron>Architect</load_neuron>

Foco principal: `category="security"`. Buscá:
- Comandos peligrosos sin gate (rm/mv/chmod/dd/redirecciones que sobrescriben).
- Secretos hardcodeados (API keys, tokens, credentials, paths a `.env`).
- Inputs externos sin validación (paths, URLs, deserialización de JSON/TOML/YAML).
- Boundaries violados (ej: layer X importando de layer Y sin razón).
- Dependencies con CVEs conocidas o sin mantener.
- Race conditions en código sync (mpsc/Mutex mal usado).

Por cada hallazgo: `<finding>` con `severity` honesta (no inflada), `file:line`, descripción del riesgo, vector de ataque si aplica, fix concreto.

Cerrá con: lo que pondría production-ready hoy vs. lo que no, y por qué.
