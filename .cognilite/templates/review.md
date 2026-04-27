Hacé una revisión arquitectónica del proyecto en el working directory.

<load_neuron>Architect</load_neuron>

Seguí el protocolo del Architect:
1. Mapeá la estructura (usá el `<project_map>` ya inyectado o `<tool>tree</tool>`).
2. Leé manifests y los 3-5 archivos top por LOC.
3. Emití un `<finding>` por cada problema concreto, con `severity`, `file:line`, `category`.
4. Cerrá con resumen ejecutivo: top-3 deudas, top-3 features, y lo que NO recomendás tocar.

Sé crítico pero accionable. Cada finding cita archivo y línea — sin citas vacías.
