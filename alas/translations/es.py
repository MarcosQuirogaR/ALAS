# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Spanish catalog.

Keys are the EXACT English strings as written in the source (config field
labels/help, figure text); values are their Spanish equivalents. A missing key
falls back to English (see :func:`alas.i18n.t`), so this file can grow
incrementally without ever breaking the UI.

Terminology follows standard Spanish aeronautical usage: ``ala`` / ``cuerda`` /
``envergadura`` / ``flecha`` / ``larguero`` / ``costilla`` / ``revestimiento`` /
``sustentación`` / ``resistencia`` / ``empuje`` / ``carga de pago`` /
``margen estático``. Established English abbreviations that Spanish-speaking
engineers use verbatim (MTOW, CL, VLM, MSES, ULD, flap, slat, spoiler, fan) are
deliberately left untranslated.
"""

from __future__ import annotations

# --- Config field labels ---------------------------------------------------
_LABELS = {
    "(Legacy, unused) CG/aero-balance mismatch weight": "(Heredado, sin uso) Peso de desajuste CG/equilibrio aerodinámico",
    "Abreast": "Asientos por fila",
    "Add center spar (root-to-kink)": "Añadir larguero central (raíz a quiebro)",
    "Additional safety factor": "Factor de seguridad adicional",
    "Aft door x": "Posición X de la puerta trasera",
    "Aileron chord fraction": "Fracción de cuerda del alerón",
    "Aileron span end": "Fin de envergadura del alerón",
    "Aileron span start": "Inicio de envergadura del alerón",
    "Aircraft type": "Tipo de aeronave",
    "Aisle width": "Anchura del pasillo",
    "Alpha sweep half-width around the trimmed design point": "Semiamplitud del barrido de alfa alrededor del punto de diseño equilibrado",
    "Alpha sweep point count": "Número de puntos del barrido de alfa",
    "Analysis": "Análisis",
    "Arrival airport": "Aeropuerto de llegada",
    "Assumed fan-face Mach number (static anchor)": "Número de Mach supuesto en la cara del fan (anclaje estático)",
    "Autobalance probe airspeed": "Velocidad de sondeo del autoequilibrado",
    "Autobalance probe alpha (high)": "Alfa de sondeo del autoequilibrado (alto)",
    "Autobalance probe alpha (low)": "Alfa de sondeo del autoequilibrado (bajo)",
    "Balanced field length factor": "Factor de longitud de campo equilibrada",
    "Belly cargo": "Carga en bodega",
    "Break-chord taper-realism penalty weight": "Peso de penalización por realismo del estrechamiento en el quiebro",
    "Business": "Business",
    "CG envelope width": "Anchura de la envolvente de CG",
    "CG-envelope compliance reward": "Recompensa por cumplir la envolvente de CG",
    "CG-envelope violation penalty weight": "Peso de penalización por violar la envolvente de CG",
    "Cabin": "Cabina",
    "Cabin preset": "Preajuste de cabina",
    "Cabin start X position": "Posición X de inicio de cabina",
    "Cabin vertical offset": "Desplazamiento vertical de la cabina",
    "Cap taper lock station": "Estación de bloqueo del estrechamiento del cordón",
    "Cap taper tip fraction": "Fracción en punta del estrechamiento del cordón",
    "Cargo": "Carga",
    "Cargo payload capacity": "Capacidad de carga de pago",
    "Center spar chord position": "Posición en cuerda del larguero central",
    "Cg trim max iterations": "Iteraciones máximas del ajuste de CG",
    "Cg trim step": "Paso del ajuste de CG",
    "Checked bag mass": "Masa de equipaje facturado",
    "Class mix mode": "Modo de mezcla de clases",
    "Cold-section ratio of specific heats": "Relación de calores específicos en la sección fría",
    "Cold-section specific heat (cp)": "Calor específico de la sección fría (cp)",
    "Combustor efficiency": "Rendimiento de la cámara de combustión",
    "Combustor pressure ratio": "Relación de presiones de la cámara de combustión",
    "Control surfaces": "Superficies de control",
    "Convergence tolerance": "Tolerancia de convergencia",
    "Core nozzle efficiency": "Rendimiento de la tobera del núcleo",
    "Core nozzle pressure ratio": "Relación de presiones de la tobera del núcleo",
    "Count": "Cantidad",
    "Cruise 1 air speed": "Velocidad de crucero 1",
    "Cruise 1 distance fraction": "Fracción de distancia del crucero 1",
    "Cruise 2 air speed": "Velocidad de crucero 2",
    "Cruise 2 distance fraction": "Fracción de distancia del crucero 2",
    "Cruise 3 air speed": "Velocidad de crucero 3",
    "Cruise 3 distance fraction": "Fracción de distancia del crucero 3",
    "Cruise Mach number": "Número de Mach de crucero",
    "Cruise altitude": "Altitud de crucero",
    "DE mutation/crossover strategy": "Estrategia de mutación/cruce de DE",
    "Departure airport": "Aeropuerto de salida",
    "Descent 1 air speed": "Velocidad de descenso 1",
    "Descent 1 altitude ft": "Altitud del descenso 1 (ft)",
    "Descent 1 rate": "Régimen de descenso 1",
    "Descent 2 air speed": "Velocidad de descenso 2",
    "Descent 2 altitude ft": "Altitud del descenso 2 (ft)",
    "Descent 2 rate": "Régimen de descenso 2",
    "Descent 3 air speed": "Velocidad de descenso 3",
    "Descent 3 altitude ft": "Altitud del descenso 3 (ft)",
    "Descent 3 rate": "Régimen de descenso 3",
    "Descent 4 air speed": "Velocidad de descenso 4",
    "Descent 4 altitude ft": "Altitud del descenso 4 (ft)",
    "Descent 4 rate": "Régimen de descenso 4",
    "Design dive speed (V_dive)": "Velocidad de picado de diseño (V_dive)",
    "Drag model": "Modelo de resistencia",
    "Drag-polar fit fallback window: CL maximum": "Ventana alternativa de ajuste de la polar: CL máximo",
    "Drag-polar fit fallback window: CL minimum": "Ventana alternativa de ajuste de la polar: CL mínimo",
    "Drag-polar fit window: CL maximum": "Ventana de ajuste de la polar: CL máximo",
    "Drag-polar fit window: CL minimum": "Ventana de ajuste de la polar: CL mínimo",
    "Economy": "Turista",
    "Elevator chord fraction": "Fracción de cuerda del timón de profundidad",
    "Elevator span end": "Fin de envergadura del timón de profundidad",
    "Elevator span start": "Inicio de envergadura del timón de profundidad",
    "Empennage": "Empenaje",
    "Enabled": "Activado",
    "Engine thrust-to-weight factor": "Factor empuje/peso del motor",
    "Exit capacity realism factor": "Factor de realismo de la capacidad por salidas",
    "Fan nozzle efficiency": "Rendimiento de la tobera del fan",
    "Fan nozzle pressure ratio": "Relación de presiones de la tobera del fan",
    "Fan polytropic efficiency": "Rendimiento politrópico del fan",
    "Fast-probe alpha (high)": "Alfa de sondeo rápido (alto)",
    "Fast-probe alpha (low)": "Alfa de sondeo rápido (bajo)",
    "Fine VLM chordwise resolution (final analysis)": "Resolución VLM fina en cuerda (análisis final)",
    "Fine VLM spanwise resolution (final analysis)": "Resolución VLM fina en envergadura (análisis final)",
    "First": "Primera",
    "Flap chord fraction": "Fracción de cuerda del flap",
    "Flap span end": "Fin de envergadura del flap",
    "Flap span start": "Inicio de envergadura del flap",
    "Forced transition x/c, lower surface": "Transición forzada x/c, intradós",
    "Forced transition x/c, upper surface": "Transición forzada x/c, extradós",
    "Frequency sweep step": "Paso del barrido en frecuencia",
    "Frequency sweep upper limit": "Límite superior del barrido en frecuencia",
    "Fuel density": "Densidad del combustible",
    "Fuel heating value": "Poder calorífico del combustible",
    "Furnishings & operations mass fraction": "Fracción de masa de mobiliario y operaciones",
    "Fuselage": "Fuselaje",
    "Fuselage cross-section count": "Número de secciones transversales del fuselaje",
    "Fuselage diameter (width)": "Diámetro del fuselaje (anchura)",
    "Fuselage height (if non-circular)": "Altura del fuselaje (si no es circular)",
    "Fuselage interference factor (Q)": "Factor de interferencia del fuselaje (Q)",
    "Fuselage wetted-area factor": "Factor de superficie mojada del fuselaje",
    "Fwd door x": "Posición X de la puerta delantera",
    "Galley count": "Número de offices",
    "Geometry": "Geometría",
    "Gravitational acceleration": "Aceleración de la gravedad",
    "Great circle points": "Puntos del círculo máximo",
    "H-stab offset forward of tail tip": "Desplazamiento del estabilizador horizontal por delante de la punta de cola",
    "H-stab root chord": "Cuerda en raíz del estabilizador horizontal",
    "H-stab root twist": "Torsión en raíz del estabilizador horizontal",
    "H-stab tip chord": "Cuerda en punta del estabilizador horizontal",
    "H-stab tip leading edge (x, y, z)": "Borde de ataque en punta del estabilizador horizontal (x, y, z)",
    "H-stab tip twist": "Torsión en punta del estabilizador horizontal",
    "H-stab vertical offset": "Desplazamiento vertical del estabilizador horizontal",
    "HPC polytropic efficiency": "Rendimiento politrópico del HPC",
    "HPT polytropic efficiency": "Rendimiento politrópico de la HPT",
    "Hot-section ratio of specific heats": "Relación de calores específicos en la sección caliente",
    "Hot-section specific heat (cp)": "Calor específico de la sección caliente (cp)",
    "Include fuselage destabilising effect": "Incluir el efecto desestabilizador del fuselaje",
    "Initial climb air speed": "Velocidad de ascenso inicial",
    "Initial climb altitude fraction": "Fracción de altitud del ascenso inicial",
    "Initial climb rate": "Régimen de ascenso inicial",
    "Inlet pressure recovery": "Recuperación de presión en la toma",
    "Instability reject cost": "Coste de rechazo por inestabilidad",
    "Insufficient wing fuel-volume penalty weight": "Peso de penalización por volumen de combustible insuficiente en el ala",
    "Invalid-design cost": "Coste de diseño no válido",
    "Korn technology factor (kappa)": "Factor tecnológico de Korn (kappa)",
    "L/D reward weight": "Peso de recompensa por L/D",
    "LPC polytropic efficiency": "Rendimiento politrópico del LPC",
    "LPC pressure-ratio split": "Reparto de relación de presiones del LPC",
    "LPT polytropic efficiency": "Rendimiento politrópico de la LPT",
    "Landing air speed": "Velocidad de aterrizaje",
    "Landing descent rate": "Régimen de descenso en aterrizaje",
    "Landing distance factor (k_land)": "Factor de distancia de aterrizaje (k_land)",
    "Landing gear": "Tren de aterrizaje",
    "Landing-gear mass fraction": "Fracción de masa del tren de aterrizaje",
    "Lavatory count": "Número de aseos",
    "Limit load factor, negative (n_lim,neg)": "Factor de carga límite, negativo (n_lim,neg)",
    "Loading strategy": "Estrategia de carga",
    "Lower deck uld": "ULD de bodega inferior",
    "MSES timeout": "Tiempo límite de MSES",
    "MSET grid density exponent (e)": "Exponente de densidad de malla de MSET (e)",
    "MSET panel count (n)": "Número de paneles de MSET (n)",
    "MSET timeout": "Tiempo límite de MSET",
    "MTOW threshold for body (centreline) main gear": "Umbral de MTOW para tren principal de fuselaje (eje central)",
    "MTOW threshold for dual nose wheels": "Umbral de MTOW para rueda de morro doble",
    "Main deck uld": "ULD de cubierta principal",
    "Main door x": "Posición X de la puerta principal",
    "Main-gear X position": "Posición X del tren principal",
    "Main-gear strut count (0 = auto)": "Número de patas del tren principal (0 = automático)",
    "Main-gear track / fuselage-diameter factor": "Factor vía del tren principal / diámetro de fuselaje",
    "Mass model": "Modelo de masas",
    "Mass per passenger": "Masa por pasajero",
    "Mass per pax": "Masa por pasajero",
    "Matching chart plot resolution": "Resolución del diagrama de adaptación",
    "Matching chart wing-loading axis: maximum": "Eje de carga alar del diagrama de adaptación: máximo",
    "Matching chart wing-loading axis: minimum": "Eje de carga alar del diagrama de adaptación: mínimo",
    "Max airspeed with flaps extended": "Velocidad máxima con flaps extendidos",
    "Max break/root chord ratio": "Relación máxima cuerda de quiebro/raíz",
    "Max generations": "Generaciones máximas",
    "Max landing weight fraction of MTOW": "Fracción máxima de peso de aterrizaje respecto al MTOW",
    "Max lateral turnover angle": "Ángulo máximo de vuelco lateral",
    "Max lift coefficient, clean (CLmax_clean)": "Coeficiente de sustentación máximo, configuración limpia (CLmax_clean)",
    "Max lift coefficient, landing (CLmax_L)": "Coeficiente de sustentación máximo, aterrizaje (CLmax_L)",
    "Max lift coefficient, take-off (CLmax_TO)": "Coeficiente de sustentación máximo, despegue (CLmax_TO)",
    "Max main-gear load fraction": "Fracción máxima de carga en el tren principal",
    "Max nose-gear load fraction": "Fracción máxima de carga en el tren de morro",
    "Max solver iterations": "Iteraciones máximas del solver",
    "Max structural payload": "Carga de pago estructural máxima",
    "Max take-off flap deflection": "Deflexión máxima de flap en despegue",
    "Max take-off weight (MTOW)": "Peso máximo al despegue (MTOW)",
    "Max wing-area penalty weight": "Peso de penalización por superficie alar máxima",
    "Max-thickness chordwise location": "Posición en cuerda del espesor máximo",
    "Maximum H-stab volume coefficient (Vh)": "Coeficiente de volumen máximo del estabilizador horizontal (Vh)",
    "Maximum V-stab volume coefficient (Vv)": "Coeficiente de volumen máximo del estabilizador vertical (Vv)",
    "Maximum cruise CL (stall guard)": "CL máximo de crucero (protección de entrada en pérdida)",
    "Maximum fuselage fineness ratio": "Esbeltez máxima del fuselaje",
    "Maximum wing area": "Superficie alar máxima",
    "Mesh chordwise points per rib": "Puntos de malla en cuerda por costilla",
    "Min exit pair spacing": "Separación mínima entre pares de salidas",
    "Min lift coefficient, clean (CLmin_clean)": "Coeficiente de sustentación mínimo, configuración limpia (CLmin_clean)",
    "Min nose-gear load fraction": "Fracción mínima de carga en el tren de morro",
    "Min wing-loading penalty weight": "Peso de penalización por carga alar mínima",
    "Minimum H-stab area fraction": "Fracción mínima de superficie del estabilizador horizontal",
    "Minimum H-stab volume coefficient (Vh)": "Coeficiente de volumen mínimo del estabilizador horizontal (Vh)",
    "Minimum V-stab area fraction": "Fracción mínima de superficie del estabilizador vertical",
    "Minimum V-stab volume coefficient (Vv)": "Coeficiente de volumen mínimo del estabilizador vertical (Vv)",
    "Minimum airfoil thickness scale": "Escala mínima de espesor del perfil",
    "Minimum fuselage length": "Longitud mínima del fuselaje",
    "Minimum physical static margin": "Margen estático físico mínimo",
    "Minimum spar web gauge": "Espesor mínimo del alma del larguero",
    "Minimum wing loading (MTOW/S)": "Carga alar mínima (MTOW/S)",
    "Minimum wing position (fraction of fuselage length)": "Posición mínima del ala (fracción de la longitud del fuselaje)",
    "Mission": "Misión",
    "Modal damping ratio": "Razón de amortiguamiento modal",
    "Mses": "MSES",
    "NASTRAN timeout per solution": "Tiempo límite de NASTRAN por solución",
    "Nacelle/pylon interference factor (Q)": "Factor de interferencia de góndola/pilón (Q)",
    "Navdata dir": "Directorio de navdata",
    "Negative-fuel penalty weight": "Peso de penalización por combustible negativo",
    "Nose vertical offset": "Desplazamiento vertical del morro",
    "Nose-gear X position": "Posición X del tren de morro",
    "Nose-gear wheel count (0 = auto)": "Número de ruedas del tren de morro (0 = automático)",
    "Number of modes to extract": "Número de modos a extraer",
    "OEI 2nd-segment climb gradient (fallback)": "Gradiente de ascenso en 2º segmento con OEI (alternativo)",
    "OEI climb configuration CL": "CL de configuración de ascenso con OEI",
    "OEI climb flap/gear drag increment": "Incremento de resistencia de flap/tren en ascenso con OEI",
    "Optimizer": "Optimizador",
    "Outboard sweep reduction": "Reducción de flecha exterior",
    "Parallel worker processes": "Procesos de trabajo en paralelo",
    "Parasite-drag (CD0) penalty weight": "Peso de penalización por resistencia parásita (CD0)",
    "Passenger": "Pasajeros",
    "Passenger count": "Número de pasajeros",
    "Payload linear density": "Densidad lineal de carga de pago",
    "Payload-shortfall penalty weight": "Peso de penalización por déficit de carga de pago",
    "Performance": "Actuaciones",
    "Pitch": "Paso entre asientos",
    "Polar sweep: alpha maximum": "Barrido de la polar: alfa máximo",
    "Polar sweep: alpha minimum": "Barrido de la polar: alfa mínimo",
    "Polar sweep: number of points": "Barrido de la polar: número de puntos",
    "Population size multiplier": "Multiplicador del tamaño de población",
    "Premium": "Turista premium",
    "Preset": "Preajuste",
    "Print progress to console": "Mostrar el progreso en consola",
    "Profile": "Perfil de misión",
    "Propulsion cycle": "Ciclo propulsivo",
    "Propulsion installation overhead": "Sobrecoste de instalación de la propulsión",
    "Propulsion mass fallback fraction": "Fracción alternativa de masa de propulsión",
    "Random seed": "Semilla aleatoria",
    "Random vibration base PSD": "PSD base de vibración aleatoria",
    "Requirements": "Requisitos",
    "Rib count override": "Número de costillas (forzado)",
    "Rib material": "Material de las costillas",
    "Rib spacing buckling coefficient": "Coeficiente de pandeo por separación de costillas",
    "Rib web thickness": "Espesor del alma de la costilla",
    "Root airfoil section": "Perfil aerodinámico en la raíz",
    "Routes dir": "Directorio de rutas",
    "Rudder chord fraction": "Fracción de cuerda del timón de dirección",
    "Rudder span end": "Fin de envergadura del timón de dirección",
    "Rudder span start": "Inicio de envergadura del timón de dirección",
    "Run normal modes (SOL 103)": "Ejecutar modos normales (SOL 103)",
    "Run random vibration (SOL 111)": "Ejecutar vibración aleatoria (SOL 111)",
    "Run sine sweep (SOL 111)": "Ejecutar barrido senoidal (SOL 111)",
    "Run static analysis (SOL 101)": "Ejecutar análisis estático (SOL 101)",
    "Seed cluster perturbation size": "Tamaño de perturbación del grupo inicial",
    "Seed search near the initial design": "Inicializar la búsqueda cerca del diseño inicial",
    "Share of cabin length [%]": "Porcentaje de la longitud de cabina [%]",
    "SimBrief overrides route airports": "SimBrief prevalece sobre los aeropuertos de la ruta",
    "Skin gauge": "Espesor del revestimiento",
    "Skin material": "Material del revestimiento",
    "Slat chord fraction": "Fracción de cuerda del slat",
    "Slat span end": "Fin de envergadura del slat",
    "Slat span start": "Inicio de envergadura del slat",
    "Slender-fuselage penalty weight": "Peso de penalización por fuselaje demasiado esbelto",
    "Solver": "Solver",
    "Spanwise integration stations": "Estaciones de integración en envergadura",
    "Spar cap material": "Material del cordón del larguero",
    "Spar chord positions": "Posiciones en cuerda de los largueros",
    "Spar web material": "Material del alma del larguero",
    "Spoiler chord fraction": "Fracción de cuerda del spoiler",
    "Spoiler span end": "Fin de envergadura del spoiler",
    "Spoiler span start": "Inicio de envergadura del spoiler",
    "Static-margin target penalty weight": "Peso de penalización por desviación del margen estático objetivo",
    "Step climb 1 air speed": "Velocidad del ascenso escalonado 1",
    "Step climb 1 altitude fraction": "Fracción de altitud del ascenso escalonado 1",
    "Step climb 1 rate": "Régimen del ascenso escalonado 1",
    "Step climb 2 air speed": "Velocidad del ascenso escalonado 2",
    "Step climb 2 rate": "Régimen del ascenso escalonado 2",
    "Stiffened-panel radius of gyration": "Radio de giro del panel rigidizado",
    "Structures": "Estructuras",
    "Strut material": "Material de la pata",
    "Suave runner dir": "Directorio del ejecutor de SUAVE",
    "Suave venv dir": "Directorio del entorno virtual de SUAVE",
    "Systems & equipment mass fraction": "Fracción de masa de sistemas y equipos",
    "Tail VLM panel count": "Número de paneles VLM de la cola",
    "Tail airfoil section": "Perfil aerodinámico de la cola",
    "Tail dynamic-pressure efficiency (eta_t)": "Rendimiento de presión dinámica en cola (eta_t)",
    "Tail vertical offset": "Desplazamiento vertical de la cola",
    "Tail-area-deficit penalty weight": "Peso de penalización por déficit de superficie de cola",
    "Tail-volume-coefficient penalty weight": "Peso de penalización por coeficiente de volumen de cola",
    "Tailcone length": "Longitud del cono de cola",
    "Takeoff air speed": "Velocidad de despegue",
    "Takeoff altitude gain": "Ganancia de altitud en despegue",
    "Takeoff climb rate": "Régimen de ascenso en despegue",
    "Target cg pct mac": "CG objetivo (% de la MAC)",
    "Target static margin": "Margen estático objetivo",
    "Texture path": "Ruta de la textura",
    "Thin-airfoil penalty weight": "Peso de penalización por perfil demasiado fino",
    "Thrust lapse ratio": "Relación de caída de empuje",
    "Timeout": "Tiempo límite",
    "Tip airfoil section": "Perfil aerodinámico en la punta",
    "Tire class": "Clase de neumático",
    "Tire load safety factor": "Factor de seguridad de carga del neumático",
    "Too-short-fuselage penalty weight": "Peso de penalización por fuselaje demasiado corto",
    "Trailing-edge rib panel mode": "Modo de panel de costilla en el borde de salida",
    "Trailing-edge strip thickness": "Espesor de la banda del borde de salida",
    "Transition N-crit": "N crítico de transición",
    "Trim-alpha penalty weight": "Peso de penalización del alfa de equilibrado",
    "Trim-alpha window: maximum": "Ventana del alfa de equilibrado: máximo",
    "Trim-alpha window: minimum": "Ventana del alfa de equilibrado: mínimo",
    "Trim-solve h-stab incidence probe delta": "Delta de sondeo de incidencia del estabilizador en el equilibrado",
    "Turbine mechanical efficiency": "Rendimiento mecánico de la turbina",
    "Ultimate load factor (n_ult)": "Factor de carga último (n_ult)",
    "Usable fuel-tank volume fraction": "Fracción utilizable del volumen de los depósitos",
    "Use main deck": "Usar cubierta principal",
    "V-stab offset forward of tail tip": "Desplazamiento del estabilizador vertical por delante de la punta de cola",
    "V-stab root chord": "Cuerda en raíz del estabilizador vertical",
    "V-stab tip chord": "Cuerda en punta del estabilizador vertical",
    "V-stab tip leading edge (x, y, z)": "Borde de ataque en punta del estabilizador vertical (x, y, z)",
    "V-stab vertical offset": "Desplazamiento vertical del estabilizador vertical",
    "VLM chordwise panel resolution": "Resolución de paneles VLM en cuerda",
    "VLM spanwise panel resolution": "Resolución de paneles VLM en envergadura",
    "Viscous drag margin": "Margen de resistencia viscosa",
    "Wall thickness": "Espesor de pared",
    "Wave-drag onset Mach": "Mach de aparición de la resistencia de onda",
    "Wave-drag rise coefficient": "Coeficiente de aumento de la resistencia de onda",
    "Weights": "Pesos",
    "Wheels per main-gear strut (0 = auto)": "Ruedas por pata del tren principal (0 = automático)",
    "Width": "Anchura",
    "Wing": "Ala",
    "Wing VLM panel count": "Número de paneles VLM del ala",
    "Wing break span location": "Posición en envergadura del quiebro del ala",
    "Wing break twist": "Torsión en el quiebro del ala",
    "Wing break vertical offset": "Desplazamiento vertical del quiebro del ala",
    "Wing interference factor (Q)": "Factor de interferencia del ala (Q)",
    "Wing root X position": "Posición X de la raíz del ala",
    "Wing root twist": "Torsión en la raíz del ala",
    "Wing root vertical offset": "Desplazamiento vertical de la raíz del ala",
    "Wing suspended-mass fraction": "Fracción de masa suspendida del ala",
    "Wing tip vertical offset": "Desplazamiento vertical de la punta del ala",
    "Wing wetted-area factor": "Factor de superficie mojada del ala",
    "Wing-root trailing-edge angle penalty weight": "Peso de penalización por ángulo del borde de salida en la raíz",
    "Wing-too-far-forward penalty weight": "Peso de penalización por ala demasiado adelantada",
    "Wingspan penalty (per metre)": "Penalización por envergadura (por metro)",
    # V-speed ratio labels are conventional symbols; kept verbatim.
    "V1 / VR": "V1 / VR",
    "V2 / VS_TO": "V2 / VS_TO",
    "VAPP / VS_land": "VAPP / VS_land",
    "VMC / VS_TO": "VMC / VS_TO",
    "VR / VMC floor": "Mínimo VR / VMC",
    "VR / VS_TO floor": "Mínimo VR / VS_TO",
    "VTD / VS_land": "VTD / VS_land",
}

# --- Config field help text ------------------------------------------------
# Long-form explanations shown when hovering a field. Regulatory references
# (CS-25, FAA AC), model names (Torenbeek, Raymer, Korn) and symbols are kept
# verbatim -- they are proper nouns to an engineer reading in either language.
_HELP = {
    "Design cruise Mach number -- the primary speed target the optimizer sizes the aircraft around.": "Número de Mach de crucero de diseño: el objetivo de velocidad principal en torno al cual el "
    "optimizador dimensiona la aeronave.",
    "Design cruise altitude, used to compute air density/speed of sound for the cruise design point.": "Altitud de crucero de diseño, empleada para calcular la densidad del aire y la velocidad del "
    "sonido en el punto de diseño de crucero.",
    "Target maximum take-off weight -- anchors the whole weight & balance / sizing pipeline.": "Peso máximo al despegue objetivo: es la referencia de todo el proceso de pesos, centrado y "
    "dimensionado.",
    "'passenger' or 'cargo' -- switches which cabin-preset list and payload model apply.": "'passenger' (pasajeros) o 'cargo' (carga): determina qué lista de preajustes de cabina y qué "
    "modelo de carga de pago se aplican.",
    "Named seating/payload layout preset ('Ryanair', 'Iberia', 'Emirates' for passenger; 'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab.": "Preajuste de configuración de asientos/carga ('Ryanair', 'Iberia', 'Emirates' para pasajeros; "
    "'Max payload', 'Dense payload' para carga). 'Custom' permite editar a mano la pestaña Cabina y "
    "carga de pago.",
    "Target passenger count (if aircraft_type is 'passenger'). Auto-recomputed when a cabin preset is active -- only editable with cabin_preset set to 'Custom'.": "Número de pasajeros objetivo (si el tipo de aeronave es 'passenger'). Se recalcula "
    "automáticamente cuando hay un preajuste de cabina activo; solo es editable con el preajuste "
    "en 'Custom'.",
    "Target cargo payload capacity (if aircraft_type is 'cargo'). Auto-recomputed when a cabin preset is active -- only editable with cabin_preset set to 'Custom'.": "Capacidad de carga de pago objetivo (si el tipo de aeronave es 'cargo'). Se recalcula "
    "automáticamente cuando hay un preajuste de cabina activo; solo es editable con el preajuste "
    "en 'Custom'.",
    "Maximum structural payload (= MZFW - OEW), i.e. the most the airframe may carry regardless of how much the belly could physically hold. In passenger mode the detailed layout fills the lower-deck belly with revenue freight (on top of passengers + checked bags) up to this structural limit, so the payload -- and therefore the residual fuel (MTOW - OEW - payload) -- matches the real aircraft's max-payload point. A widebody belly can volumetrically hold far more than this structural cap, so without it 'fill the belly' overshoots. 0 = disabled (use the explicit Cabin & Payload belly_cargo_kg instead).": "Carga de pago estructural máxima (= MZFW - OEW), es decir, lo máximo que la célula puede "
    "transportar con independencia de lo que quepa físicamente en la bodega. En modo pasajeros, la "
    "configuración detallada llena la bodega inferior con carga de pago (además de pasajeros y "
    "equipaje facturado) hasta este límite estructural, de modo que la carga -- y por tanto el "
    "combustible restante (MTOW - OEW - carga) -- coincide con el punto de carga máxima del avión "
    "real. La bodega de un fuselaje ancho admite en volumen mucho más que este tope estructural, "
    "así que sin él 'llenar la bodega' se pasaría de largo. 0 = desactivado (usa entonces el campo "
    "belly_cargo_kg de Cabina y carga de pago).",
    "Limit load factor times the 1.5 safety margin, fed into the Torenbeek structural mass formulas.": "Factor de carga límite multiplicado por el margen de seguridad de 1,5; alimenta las fórmulas "
    "de masa estructural de Torenbeek.",
    "Structural design dive speed, fed into the Torenbeek structural mass formulas. Also VD on the V-n diagram; design cruise speed VC is derived as VD/1.25 (CS-25.335(b) minimum margin) rather than a separate field.": "Velocidad de picado de diseño estructural; alimenta las fórmulas de masa estructural de "
    "Torenbeek. Es también la VD del diagrama V-n; la velocidad de crucero de diseño VC se deduce "
    "como VD/1,25 (margen mínimo de CS-25.335(b)) en lugar de ser un campo independiente.",
    "CS-25.337(c) negative limit load factor for the V-n diagram. The positive limit load factor is derived as ultimate_load_factor / 1.5 (CS-25.303) rather than a separate field.": "Factor de carga límite negativo según CS-25.337(c) para el diagrama V-n. El factor de carga "
    "límite positivo se deduce como ultimate_load_factor / 1,5 (CS-25.303) en lugar de ser un "
    "campo independiente.",
    "Upper bound on wing planform area; the optimizer is penalised for exceeding it.": "Límite superior de la superficie alar en planta; el optimizador es penalizado si lo supera.",
    "Lower bound on wing loading (MTOW / wing area) -- keeps the wing from being sized too large for the mass it carries.": "Límite inferior de la carga alar (MTOW / superficie alar): evita que el ala resulte demasiado "
    "grande para la masa que soporta.",
    "Candidate designs whose required cruise CL exceeds this are rejected as infeasible (too close to stall).": "Los diseños candidatos cuyo CL de crucero requerido supere este valor se rechazan por "
    "inviables (demasiado cerca de la entrada en pérdida).",
    "Static margin at the Aft CG Limit: Aft CG Limit (%MAC) = Neutral Point (%MAC) - target_static_margin*100. A positive value ensures positive static stability when the CG is at the aft limit.": "Margen estático en el límite trasero de CG: límite trasero (%MAC) = punto neutro (%MAC) - "
    "target_static_margin*100. Un valor positivo garantiza estabilidad estática positiva con el CG "
    "en el límite trasero.",
    "Width of the CG envelope. Forward CG Limit (%MAC) = Aft CG Limit (%MAC) - cg_range_pct_mac.": "Anchura de la envolvente de CG. Límite delantero (%MAC) = límite trasero (%MAC) - "
    "cg_range_pct_mac.",
    "Minimum static margin measured using the actual mass-model (physical) CG, not the aerodynamic reference point. Designs below this are hard-rejected as inherently unstable. 0.0 = bare stability; 0.05 = 5% MAC buffer (recommended).": "Margen estático mínimo medido con el CG físico real del modelo de masas, no con el punto de "
    "referencia aerodinámico. Los diseños por debajo se rechazan por ser intrínsecamente "
    "inestables. 0,0 = estabilidad justa; 0,05 = margen del 5 % de la MAC (recomendado).",
    "Combined average mass per occupant (body + baggage). FAA AC 120-27E standard is 100 kg; airlines may use 90-105 kg.": "Masa media combinada por ocupante (persona + equipaje). El estándar FAA AC 120-27E es 100 kg; "
    "las aerolíneas suelen emplear entre 90 y 105 kg.",
    # Cabin & payload -- the page reworked to percentage-based class mixes.
    "'percent': give each class a share of cabin length and let the layout solve the seat counts (recommended -- counts depend on pitch, abreast and fuselage shape). 'count': type exact per-class seat numbers instead.": "'percent': asigna a cada clase un porcentaje de la longitud de cabina y deja que el cálculo de "
    "distribución determine el número de asientos (recomendado: el número depende del paso, los "
    "asientos por fila y la forma del fuselaje). 'count': introduce directamente el número exacto "
    "de asientos por clase.",
    "Percentage of usable cabin floor length allocated to this class. Seat count is derived from it using this class's pitch/abreast and the real fuselage geometry. Shares are normalised, so they need not add up to exactly 100. Set the class to 0 to remove it. Only used when Class mix mode is 'percent'.": "Porcentaje de la longitud útil de cabina asignado a esta clase. El número de asientos se "
    "deduce de él usando el paso y los asientos por fila de la clase junto con la geometría real "
    "del fuselaje. Los porcentajes se normalizan, así que no tienen por qué sumar exactamente 100. "
    "Pon la clase a 0 para eliminarla. Solo se usa con el modo de mezcla de clases en 'percent'.",
    "Exact number of seats in this class (0 = class absent). Only editable when Class mix mode is 'count'; in 'percent' mode this is computed from the share above.": "Número exacto de asientos de esta clase (0 = clase ausente). Solo editable con el modo de "
    "mezcla de clases en 'count'; en modo 'percent' se calcula a partir del porcentaje anterior.",
    "Longitudinal seat spacing. Regulatory/industry floor is economy's 28 in (0.7112 m); premium/business/first cabins use larger values (Matrix B, payload_processed.md).": "Separación longitudinal entre asientos. El mínimo normativo y de la industria es el de turista, "
    "28 in (0,7112 m); las cabinas premium, business y primera emplean valores mayores (Matriz B, "
    "payload_processed.md).",
    "Lateral seat footprint (incl. shell/armrests -- wider than the raw cushion for premium classes). Floor is economy's 16 in (0.4064 m) cushion width (Matrix B).": "Huella lateral del asiento (incluye carcasa y reposabrazos, por lo que es mayor que el propio "
    "cojín en las clases premium). El mínimo es la anchura de cojín de turista, 16 in (0,4064 m) "
    "(Matriz B).",
    # Mission / routing.
    "When your most recent SimBrief OFP is for a different city pair than the departure/arrival airports selected above, fly the OFP's pair instead. A real dispatched OFP (real SID/STAR/airways, current AIRAC) is the most accurate route ALAS can get, so it takes precedence. Turn off to keep the manually-selected airports and ignore a mismatched OFP.": "Cuando tu OFP más reciente de SimBrief corresponde a un par de aeropuertos distinto del "
    "seleccionado arriba, se vuela el par del OFP. Un OFP realmente despachado (SID/STAR y "
    "aerovías reales, AIRAC vigente) es la ruta más precisa que ALAS puede obtener, por lo que "
    "tiene prioridad. Desactívalo para conservar los aeropuertos elegidos manualmente e ignorar un "
    "OFP que no coincida.",
    # --- Geometry scaffold --------------------------------------------------
    "Main-wing scaffold parameters not already covered by the optimizer's design vector.": "Parámetros del esquema del ala principal que no forman parte del vector de diseño del optimizador.",
    "Fuselage-station X of the wing-root leading-edge datum -- how far aft of the nose the wing sits.": "Estación X del fuselaje del borde de ataque en la raíz del ala: a qué distancia del morro se sitúa el ala.",
    "Vertical (Z) placement of the wing-root leading edge relative to the fuselage centerline.": "Posición vertical (Z) del borde de ataque en la raíz del ala respecto al eje del fuselaje.",
    "Vertical placement of the mid-span 'break' section leading edge, where the taper/dihedral rate changes.": "Posición vertical del borde de ataque en la sección de quiebro a media envergadura, donde cambia el "
    "estrechamiento o el diedro.",
    "Vertical placement of the wing-tip leading edge. Tip above root gives positive dihedral.": "Posición vertical del borde de ataque en la punta del ala. Una punta por encima de la raíz da diedro positivo.",
    "Geometric twist (incidence) of the root section, positive = leading-edge-up (washin).": "Torsión geométrica (incidencia) de la sección de raíz; positiva = borde de ataque hacia arriba (washin).",
    "Geometric twist of the mid-span break section.": "Torsión geométrica de la sección de quiebro a media envergadura.",
    "Spanwise position of the trailing-edge break, as a fraction of the semispan (0 = root, 1 = tip).": "Posición en envergadura del quiebro del borde de salida, como fracción de la semienvergadura "
    "(0 = raíz, 1 = punta).",
    "How many degrees less swept the outboard panel is than the inboard panel (a common yehudi/crank shape).": "Cuántos grados menos de flecha tiene el panel exterior respecto al interior (la típica forma de "
    "yehudi o quiebro).",
    "Reference airfoil at the wing root, morphed by the design vector's thickness/camber scale factors.": "Perfil aerodinámico de referencia en la raíz del ala, deformado por los factores de escala de "
    "espesor y curvatura del vector de diseño.",
    "Reference airfoil at the wing tip.": "Perfil aerodinámico de referencia en la punta del ala.",
    "Spanwise panel refinement per wing section for the vortex-lattice solver. Higher = more accurate, slower.": "Refinamiento de paneles en envergadura por sección del ala para el método de red de torbellinos. "
    "Más alto = más preciso pero más lento.",
    "Horizontal and vertical stabiliser scaffold.": "Esquema de los estabilizadores horizontal y vertical.",
    "Reference airfoil shared by both the horizontal and vertical stabilisers (usually symmetric, e.g. NACA 00xx).": "Perfil aerodinámico compartido por los estabilizadores horizontal y vertical (normalmente simétrico, "
    "p. ej. NACA 00xx).",
    # --- Empennage ----------------------------------------------------------
    "Spanwise panel refinement per tail surface for the vortex-lattice solver.": "Refinamiento de paneles en envergadura por superficie de cola para el método de red de torbellinos.",
    "How far forward of the fuselage tail tip the horizontal-stabiliser root leading edge sits.": "Distancia por delante de la punta de cola del fuselaje a la que se sitúa el borde de ataque de la raíz "
    "del estabilizador horizontal.",
    "Vertical placement of the horizontal stabiliser relative to the fuselage centerline.": "Posición vertical del estabilizador horizontal respecto al eje del fuselaje.",
    "Incidence of the horizontal stabiliser root -- usually slightly negative to trim the wing's nose-down pitching moment.": "Incidencia de la raíz del estabilizador horizontal: normalmente algo negativa para equilibrar el momento "
    "de picado del ala.",
    "Position of the horizontal-stabiliser tip leading edge relative to its root, as (x, y, z).": "Posición del borde de ataque en la punta del estabilizador horizontal respecto a su raíz, como (x, y, z).",
    "How far forward of the fuselage tail tip the vertical-stabiliser root leading edge sits.": "Distancia por delante de la punta de cola del fuselaje a la que se sitúa el borde de ataque de la raíz "
    "del estabilizador vertical.",
    "Position of the vertical-stabiliser tip leading edge relative to its root, as (x, y, z).": "Posición del borde de ataque en la punta del estabilizador vertical respecto a su raíz, como (x, y, z).",
    # --- Fuselage -----------------------------------------------------------
    "Fuselage body-of-revolution stations, incl. diameter (width) and length breakdown.": "Estaciones del fuselaje como sólido de revolución, incluyendo diámetro (anchura) y reparto de longitudes.",
    "Maximum fuselage cross-sectional diameter -- the main driver of cabin width and wetted area. Best judged on the 3-view preview's front/isometric panels, not the top view.": "Diámetro máximo de la sección transversal del fuselaje: el factor principal de la anchura de cabina y de "
    "la superficie mojada. Se aprecia mejor en las vistas frontal e isométrica de la previsualización de tres "
    "vistas que en la vista en planta.",
    "Vertical cross-section dimension. Leave blank/none for a circular fuselage where height equals diameter.": "Dimensión vertical de la sección. Déjalo en blanco para un fuselaje circular en el que la altura coincide "
    "con el diámetro.",
    "Vertical offset of the nose tip relative to the fuselage centerline.": "Desplazamiento vertical de la punta del morro respecto al eje del fuselaje.",
    "X-station where the fuselage first reaches full diameter, i.e. the end of the nose taper.": "Estación X en la que el fuselaje alcanza por primera vez su diámetro máximo, es decir, el final del "
    "afinamiento del morro.",
    "Vertical offset of the cylindrical cabin section relative to the fuselage centerline.": "Desplazamiento vertical de la sección cilíndrica de cabina respecto al eje del fuselaje.",
    "Length of the aft taper, from the end of the cylindrical cabin section to the tail tip.": "Longitud del afinamiento trasero, desde el final de la sección cilíndrica de cabina hasta la punta de cola.",
    "Vertical offset of the upswept tail tip (positive = tail rises above centerline, typical for ground clearance/rotation).": "Desplazamiento vertical de la punta de cola elevada (positivo = la cola sube por encima del eje, lo "
    "habitual para dar guarda al suelo en la rotación).",
    "Number of longitudinal stations used to loft the fuselage body. Higher = smoother surface, slower to draw.": "Número de estaciones longitudinales usadas para generar la superficie del fuselaje. Más alto = superficie "
    "más suave pero más lenta de dibujar.",
    # --- Engine / nacelle ---------------------------------------------------
    "Podded engine / nacelle placement, shape, and design parameters -- edited on the dedicated 'Engine Designer' Advanced Settings tab, not here.": "Posición, forma y parámetros de diseño del motor en góndola: se editan en la pestaña 'Diseñador de motor' "
    "de los ajustes avanzados, no aquí.",
    "Name from the built-in engine database (see the Engine selector on the Inputs tab) -- drives thrust, mass, and the default nacelle profile.": "Nombre de la base de datos de motores incorporada (ver el selector de motor en la pestaña Entradas): "
    "determina el empuje, la masa y el perfil de góndola por defecto.",
    "List of (x-station [m], radius-fraction [0-1 of radius_scale_m below]) pairs tracing the nacelle's longitudinal silhouette from inlet (x=0) to exit -- see the Engine Designer tab's live nacelle-silhouette preview for a picture of the shape these points draw. Auto-filled from the engine database when engine_name is recognised.": "Lista de pares (estación x [m], fracción de radio [0-1 respecto a radius_scale_m]) que trazan la silueta "
    "longitudinal de la góndola desde la entrada (x=0) hasta la salida; consulta la previsualización de la "
    "silueta en la pestaña Diseñador de motor para ver la forma que dibujan estos puntos. Se rellena "
    "automáticamente desde la base de datos cuando se reconoce el nombre del motor.",
    "Physical radius the nacelle_profile's fraction-of-1.0 points are scaled by.": "Radio físico por el que se escalan los puntos (expresados como fracción de 1,0) del perfil de la góndola.",
    "Y-coordinate of each engine's mount position (negative = left/port side); one entry per engine.": "Coordenada Y de la posición de montaje de cada motor (negativa = lado izquierdo); una entrada por motor.",
    "Vertical placement of the engine centerline relative to the wing reference plane (negative = below the wing).": "Posición vertical del eje del motor respecto al plano de referencia del ala (negativa = por debajo del ala).",
    "How far forward of the (swept) wing leading edge the nacelle inlet face sits.": "Distancia por delante del borde de ataque (en flecha) del ala a la que se sitúa la cara de entrada de la "
    "góndola.",
    "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, the Matching Chart T/W lookup, and the SUAVE turbofan sizing target.": "Empuje máximo de despegue a nivel del mar en estático, por motor. Determina la masa de propulsión, el "
    "valor de T/W del diagrama de adaptación y el objetivo de dimensionado del turbofán en SUAVE.",
    "Ratio of bypass (fan duct) to core mass flow. Feeds the SUAVE turbofan network and the Propulsion Analysis on-design cycle.": "Relación entre el gasto másico derivado (conducto del fan) y el del núcleo. Alimenta la red de turbofán de "
    "SUAVE y el ciclo de diseño del Análisis de propulsión.",
    "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the fan). Feeds SUAVE's compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) and the Propulsion Analysis cycle's compressor_pressure_ratio.": "Relación de presiones total a través de los compresores del núcleo (LPC x HPC combinados, SIN incluir el "
    "fan). Alimenta el dimensionado de compresores de SUAVE (dividido en una relación fija de LPC más una de "
    "HPC resuelta) y la relación de presiones del ciclo del Análisis de propulsión.",
    "Pressure ratio across the fan (bypass stream), separate from the core OPR above.": "Relación de presiones a través del fan (flujo derivado), independiente de la OPR del núcleo anterior.",
    "Combustor-exit stagnation temperature -- the primary driver of specific thrust and thermal efficiency in the on-design cycle.": "Temperatura de remanso a la salida de la cámara de combustión: el factor principal del empuje específico y "
    "del rendimiento térmico en el ciclo de diseño.",
    "Reference cruise thrust-specific fuel consumption, used by the Breguet payload-range diagram. Not necessarily identical to the Propulsion Analysis tab's on-design-cycle-computed TSFC (a fast conceptual cycle model with generic component efficiencies vs. this field's real/published in-service figure) -- see that tab's caption.": "Consumo específico de combustible de referencia en crucero, empleado por el diagrama carga-alcance de "
    "Breguet. No tiene por qué coincidir con el TSFC calculado por el ciclo de diseño en la pestaña Análisis de "
    "propulsión (un modelo conceptual rápido con rendimientos genéricos, frente a la cifra real publicada de "
    "este campo); consulta la nota de esa pestaña.",
    "Fan face diameter -- informational/reference only (does not currently size the nacelle profile, which comes from radius_scale_m/nacelle_profile above).": "Diámetro de la cara del fan: solo informativo (actualmente no dimensiona el perfil de la góndola, que "
    "procede de radius_scale_m y nacelle_profile).",
    # --- Drag model ---------------------------------------------------------
    "Ratio of wetted (exposed, both sides) surface area to planform area for a thin wing. Used by the drag buildup.": "Relación entre la superficie mojada (expuesta, ambas caras) y la superficie en planta de un ala delgada. "
    "La emplea el desglose de resistencia.",
    "Correction factor on pi*diameter*length for a non-cylindrical (tapered nose/tail) fuselage body.": "Factor de corrección sobre pi*diámetro*longitud para un fuselaje no cilíndrico (con morro y cola "
    "afinados).",
    "Chordwise location of maximum airfoil thickness (as a fraction of chord), used in the wing form factor.": "Posición en cuerda del espesor máximo del perfil (como fracción de la cuerda), empleada en el factor de "
    "forma del ala.",
    "How much the wing-fuselage junction disturbs the local flow, multiplying the wing's parasite drag. 1.0 = no interference.": "Cuánto perturba la unión ala-fuselaje el flujo local, multiplicando la resistencia parásita del ala. "
    "1,0 = sin interferencia.",
    "How much neighbouring components (wing, tail) disturb the fuselage's flow, multiplying its parasite drag.": "Cuánto perturban los componentes vecinos (ala, cola) el flujo del fuselaje, multiplicando su resistencia "
    "parásita.",
    "Lumped multiplier applied to the total parasite drag, accounting for excrescences, gaps and roughness not captured component-by-component. 1.10 = +10% margin.": "Multiplicador global aplicado a la resistencia parásita total, que recoge protuberancias, juntas y "
    "rugosidad no contabilizadas componente a componente. 1,10 = +10 % de margen.",
    "How much the engine nacelle and pylon disturb the local flow. Typical value 1.3 for podded under-wing installations (Raymer).": "Cuánto perturban la góndola y el pilón el flujo local. Valor típico 1,3 para instalaciones en góndola bajo "
    "el ala (Raymer).",
    "Airfoil-technology factor in the Korn wave-drag equation; ~0.95 for modern supercritical sections, lower for older/less efficient sections.": "Factor tecnológico del perfil en la ecuación de resistencia de onda de Korn; ~0,95 para perfiles "
    "supercríticos modernos y menor para perfiles antiguos o menos eficientes.",
    "Below this Mach number, transonic wave drag is assumed zero (not yet computed).": "Por debajo de este número de Mach se supone resistencia de onda transónica nula (aún no se calcula).",
    "Leading constant in the Korn wave-drag rise: CD_wave = coefficient * (M - M_drag_divergence)^4.": "Constante principal del aumento de resistencia de onda de Korn: "
    "CD_onda = coeficiente * (M - M_divergencia)^4.",
    # --- Optimizer objective weights ---------------------------------------
    "Primary reward multiplier on L/D. Raise to prioritize aerodynamic efficiency over all penalties below.": "Multiplicador principal de recompensa sobre L/D. Auméntalo para priorizar la eficiencia aerodinámica "
    "frente a todas las penalizaciones siguientes.",
    "Penalizes cruise trim alpha outside [alpha_min_penalty_deg, alpha_max_penalty_deg]. Zero cost inside the window, quadratic outside. Kept small so the optimizer focuses on L/D.": "Penaliza un alfa de equilibrado en crucero fuera de [alpha_min_penalty_deg, alpha_max_penalty_deg]. Coste "
    "nulo dentro de la ventana y cuadrático fuera. Se mantiene pequeño para que el optimizador se centre en L/D.",
    "Lower bound of the acceptable cruise trim-alpha window (no penalty above this).": "Límite inferior de la ventana aceptable del alfa de equilibrado en crucero (sin penalización por encima).",
    "Upper bound of the acceptable cruise trim-alpha window (no penalty below this).": "Límite superior de la ventana aceptable del alfa de equilibrado en crucero (sin penalización por debajo).",
    "Small linear penalty per metre of span -- discourages excessively large wings.": "Pequeña penalización lineal por metro de envergadura: desincentiva alas excesivamente grandes.",
    "Rewards lower parasite drag (CD0); a thinner/cleaner shape reduces this term's cost. Typical cruise CD0 is 0.016-0.020.": "Recompensa una menor resistencia parásita (CD0); una forma más fina y limpia reduce el coste de este "
    "término. El CD0 típico en crucero es 0,016-0,020.",
    "Penalty per m^2 the wing area exceeds requirements.max_wing_area_m2.": "Penalización por cada m² en que la superficie alar supera requirements.max_wing_area_m2.",
    "Penalty per (kg/m^2)^2 below requirements.min_wing_loading_kg_m2.": "Penalización por cada (kg/m²)² por debajo de requirements.min_wing_loading_kg_m2.",
    "NOT read by the cost function (objective.py) -- kept only so old saved YAML configs referencing this key still load without error. Originally intended to penalize (Delta x_cg / MAC)^2, but that formula was never actually wired up; the field's real runtime effect was fully redundant with static_margin_penalty_scale, since both applied to the identical static-margin-vs-target term, so the two are consolidated into that single, correctly-named, appropriately-soft term. Physical CG-envelope compliance is enforced separately by cg_envelope_penalty_scale/cg_envelope_reward below.": "La función de coste (objective.py) NO lee este campo: se conserva únicamente para que configuraciones YAML "
    "antiguas que lo referencien sigan cargando sin error. En origen pretendía penalizar (Δx_cg / MAC)², pero "
    "esa fórmula nunca llegó a conectarse; su efecto real en ejecución era totalmente redundante con "
    "static_margin_penalty_scale (ambos actuaban sobre el mismo término de margen estático frente al objetivo) "
    "hasta que se unificaron en ese único término, mejor nombrado y adecuadamente suave. El cumplimiento de la "
    "envolvente física de CG se controla por separado mediante cg_envelope_penalty_scale y cg_envelope_reward.",
    "Penalizes the physical CG exceeding the [fwd, aft] CG envelope limits (% MAC, from Design Requirements). At 5% MAC beyond the limit the cost is comparable to one L/D unit, rising steeply further out.": "Penaliza que el CG físico sobrepase los límites [delantero, trasero] de la envolvente de CG (% MAC, de los "
    "requisitos de diseño). A un 5 % de MAC más allá del límite el coste equivale a una unidad de L/D, y crece "
    "bruscamente a partir de ahí.",
    "Reward applied to cost if the aircraft's entire operational CG envelope is within limits.": "Recompensa aplicada al coste si toda la envolvente operativa de CG de la aeronave queda dentro de límites.",
    "Penalizes negative fuel mass (OEW + payload exceeding MTOW), normalized by MTOW.": "Penaliza una masa de combustible negativa (OEW más carga de pago por encima del MTOW), normalizada por el "
    "MTOW.",
    "Penalizes the wing's physical usable fuel-tank volume (physics.performance.wing_fuel_volume_m3, Torenbeek geometric estimate) being too small to hold the fuel mass the weight & balance analysis says this design actually needs -- a wing that's too thin/small/tapered to carry its own required fuel is not a buildable aircraft, independent of whether the MTOW fuel-mass budget itself closes. Quadratic on the fractional shortfall (required_fuel - tank_capacity) / required_fuel.": "Penaliza que el volumen útil real de los depósitos del ala (estimación geométrica de Torenbeek) sea "
    "insuficiente para la masa de combustible que el análisis de pesos y centrado indica que el diseño "
    "necesita: un ala demasiado fina, pequeña o estrechada como para alojar su propio combustible no es un "
    "avión construible, con independencia de que el balance de masa dentro del MTOW cuadre. Cuadrática sobre el "
    "déficit relativo (combustible_necesario - capacidad) / combustible_necesario.",
    "SOFT preference nudging compliant-but-suboptimal candidates toward requirements.target_static_margin -- NOT a hard requirement (the physical floor, min_physical_static_margin, and the CG-envelope itself are enforced separately and are what actually keep a design safe/legal). Kept deliberately small relative to -L/D (typically 15-25) so this doesn't crowd out genuine aerodynamic improvements: raising it much above ~20-30 risks the optimizer chasing an exact SM match instead of exploring shape space, overwhelming the L/D signal the search is meant to prioritize.": "Preferencia SUAVE que empuja a los candidatos válidos pero subóptimos hacia "
    "requirements.target_static_margin. NO es un requisito estricto: el mínimo físico "
    "(min_physical_static_margin) y la propia envolvente de CG se imponen por separado y son los que realmente "
    "mantienen el diseño seguro y certificable. Se mantiene deliberadamente pequeño frente a -L/D (típicamente "
    "15-25) para que no desplace mejoras aerodinámicas reales: subirlo mucho por encima de ~20-30 arriesga que "
    "el optimizador persiga un margen estático exacto en lugar de explorar formas, dominando la señal de L/D "
    "que la búsqueda debe priorizar.",
    "Minimum allowed airfoil thickness scale (relative to the reference section) before the thickness penalty kicks in.": "Escala mínima admisible de espesor del perfil (respecto al perfil de referencia) antes de que actúe la "
    "penalización por espesor.",
    "Penalty weight applied when the morphed airfoil thickness collapses below thickness_floor.": "Peso de penalización aplicado cuando el espesor del perfil deformado cae por debajo de thickness_floor.",
    "Minimum allowed fuselage length before the too-short-fuselage penalty kicks in.": "Longitud mínima admisible del fuselaje antes de que actúe la penalización por fuselaje demasiado corto.",
    "Penalty weight applied when the fuselage shrinks below fuselage_floor_m.": "Peso de penalización aplicado cuando el fuselaje se reduce por debajo de fuselage_floor_m.",
    "Minimum allowed horizontal-stabiliser area as a fraction of wing area. Typical transports: ~20-30%. Prevents a 'tiny tail on a huge moment arm' cheat.": "Superficie mínima admisible del estabilizador horizontal como fracción de la superficie alar. Transportes "
    "típicos: ~20-30 %. Evita la trampa de 'una cola diminuta con un brazo enorme'.",
    "Minimum allowed vertical-stabiliser area as a fraction of wing area. Typical transports: ~8-14%.": "Superficie mínima admisible del estabilizador vertical como fracción de la superficie alar. Transportes "
    "típicos: ~8-14 %.",
    "Penalty weight for tail area fractions below their minimums (stiff quadratic).": "Peso de penalización para superficies de cola por debajo de sus mínimos (cuadrática rígida).",
    "Lower bound on the horizontal-tail volume coefficient Vh = Sh*Lh/(S*c_bar), which captures tail effectiveness accounting for its moment arm, not just area (Etkin/Reid convention).": "Límite inferior del coeficiente de volumen de cola horizontal Vh = Sh*Lh/(S*c_bar), que mide la eficacia "
    "de la cola teniendo en cuenta su brazo de momento y no solo su superficie (convenio de Etkin/Reid).",
    "Upper bound on Vh -- penalises an oversized tail / an unnecessarily stretched fuselage moment arm.": "Límite superior de Vh: penaliza una cola sobredimensionada o un brazo de momento innecesariamente largo.",
    "Lower bound on the vertical-tail volume coefficient Vv = Sv*Lv/(S*b).": "Límite inferior del coeficiente de volumen de cola vertical Vv = Sv*Lv/(S*b).",
    "Upper bound on Vv (typical jet-transport max is ~0.12).": "Límite superior de Vv (el máximo típico en un reactor de transporte es ~0,12).",
    "Quadratic penalty weight for Vh/Vv falling outside their [min, max] bounds.": "Peso de la penalización cuadrática cuando Vh o Vv quedan fuera de sus límites [mín, máx].",
    "Upper bound on break_chord_m / root_chord_m. Real transport wings taper noticeably from root to the yehudi break (typically 0.45-0.65); without this bound the optimizer can inflate the break chord toward the root chord to enlarge MAC (c_ref) 'for free', which cheapens every %MAC-normalised penalty (CG envelope, static-margin target) without a real stability improvement. Soft penalty above this ratio, not a hard bound.": "Límite superior de cuerda_quiebro / cuerda_raíz. Las alas de transporte reales se estrechan notablemente "
    "de la raíz al quiebro (típicamente 0,45-0,65); sin este límite el optimizador puede inflar la cuerda del "
    "quiebro hacia la de raíz para agrandar la MAC 'gratis', lo que abarata toda penalización normalizada en "
    "%MAC (envolvente de CG, margen estático objetivo) sin mejorar realmente la estabilidad. Penalización suave "
    "por encima de esta relación, no un límite estricto.",
    "Penalty weight applied when break_chord_m / root_chord_m exceeds max_break_root_chord_ratio.": "Peso de penalización aplicado cuando cuerda_quiebro / cuerda_raíz supera max_break_root_chord_ratio.",
    "Penalizes the wing's root-to-break trailing edge (seen in planform) making an angle greater than 90 deg with the fuselage centerline -- i.e. the break station's trailing edge sitting forward of the root's. That creates a reflex (concave) corner at the wing-fuselage junction: a severe stress concentration no real transport-category wing root has, caused by a short root chord combined with a comparatively long break chord and/or too little sweep. Quadratic on the angle exceedance beyond 90 deg.": "Penaliza que el borde de salida entre la raíz y el quiebro (visto en planta) forme un ángulo mayor de 90° "
    "con el eje del fuselaje, es decir, que el borde de salida del quiebro quede por delante del de la raíz. "
    "Eso genera una esquina cóncava en la unión ala-fuselaje: una concentración de tensiones severa que ningún "
    "ala de transporte real presenta, provocada por una cuerda de raíz corta junto con una cuerda de quiebro "
    "comparativamente larga o una flecha insuficiente. Cuadrática sobre el exceso de ángulo más allá de 90°.",
    "Wing-root leading edge must sit at least this fraction of fuselage length aft of the nose -- prevents the optimizer placing the wing in the cockpit. Typical transports: 25-55%.": "El borde de ataque de la raíz del ala debe situarse al menos a esta fracción de la longitud del fuselaje "
    "por detrás del morro: evita que el optimizador coloque el ala en la cabina de vuelo. Transportes típicos: "
    "25-55 %.",
    "Penalty weight applied when the wing sits forward of min_wing_position_fraction.": "Peso de penalización aplicado cuando el ala queda por delante de min_wing_position_fraction.",
    "Steep quadratic penalty on the fractional shortfall vs. the target passenger count / cargo payload -- guides the optimizer to grow the fuselage long enough to actually fit the requested payload.": "Penalización cuadrática acusada sobre el déficit relativo frente al número de pasajeros o la carga "
    "objetivo: guía al optimizador para alargar el fuselaje lo suficiente como para alojar realmente la carga "
    "solicitada.",
    "Maximum allowed fuselage length/diameter ratio before the too-slender-fuselage penalty kicks in.": "Relación longitud/diámetro máxima admisible del fuselaje antes de que actúe la penalización por fuselaje "
    "demasiado esbelto.",
    "Penalty weight applied when the fuselage fineness ratio exceeds fineness_ratio_max.": "Peso de penalización aplicado cuando la esbeltez del fuselaje supera fineness_ratio_max.",
    "Cost returned for any candidate design that raises an exception or fails a hard guard during evaluation.": "Coste devuelto para cualquier diseño candidato que lance una excepción o incumpla una comprobación "
    "estricta durante la evaluación.",
    "Severity scale for the static-margin-floor penalty applied to a candidate that builds and analyses successfully but is rejected as physically invalid (static margin below requirements.min_physical_static_margin). NOT a flat returned cost -- this floor is graduated, not an early return (objective.py's `(deficit * severity)**3` term, where this field sets `severity` so raising/lowering it steepens/relaxes the penalty without editing code; the default reproduces the exact cubic constant used before this field was wired up). Kept distinct from failure_cost so run diagnostics can tell 'geometry/analysis crashed' apart from 'physically unstable' rejects (see OptimizationHistory.reject_reason_counts).": "Escala de severidad de la penalización por margen estático mínimo aplicada a un candidato que se construye "
    "y analiza correctamente pero se rechaza por ser físicamente inválido (margen estático por debajo de "
    "min_physical_static_margin). NO es un coste fijo devuelto: este mínimo es graduado, no una salida "
    "anticipada (el término `(déficit * severidad)**3` de objective.py, donde este campo fija la severidad, de "
    "modo que subirlo o bajarlo endurece o relaja la penalización sin tocar código). Se mantiene distinto de "
    "failure_cost para que el diagnóstico de la ejecución pueda distinguir 'fallo de geometría o análisis' de "
    "'rechazo por inestabilidad física'.",
    # --- Solver settings ----------------------------------------------------
    "SciPy differential_evolution strategy name (e.g. 'best1bin', 'rand1bin', 'best2bin') -- controls how new candidate designs are generated from the population each generation.": "Nombre de la estrategia de differential_evolution de SciPy (p. ej. 'best1bin', 'rand1bin', 'best2bin'): "
    "controla cómo se generan los nuevos diseños candidatos a partir de la población en cada generación.",
    "Maximum number of generations (iterations) the solver runs before stopping.": "Número máximo de generaciones (iteraciones) que ejecuta el solver antes de detenerse.",
    "Population size as a multiplier on the number of design variables -- more candidates per generation explores more broadly but costs more evaluations.": "Tamaño de población como multiplicador del número de variables de diseño: más candidatos por generación "
    "exploran más ampliamente pero cuestan más evaluaciones.",
    "Relative tolerance for convergence; the solver stops early once the population's cost spread falls below this.": "Tolerancia relativa de convergencia; el solver se detiene antes de tiempo cuando la dispersión de coste de "
    "la población cae por debajo de este valor.",
    "Set an integer for a reproducible run (same seed -> same result); leave blank for a different search each run.": "Introduce un entero para una ejecución reproducible (misma semilla, mismo resultado); déjalo en blanco "
    "para una búsqueda distinta en cada ejecución.",
    "Number of worker processes for parallel evaluation (>1 uses multiprocessing). Requires a picklable objective -- already the case for ALAS's optimizer.": "Número de procesos de trabajo para la evaluación en paralelo (>1 usa multiprocessing). Requiere una "
    "función objetivo serializable, algo que el optimizador de ALAS ya cumple.",
    "Print a one-line progress summary (valid count, best L/D so far) after each generation.": "Muestra un resumen de progreso de una línea (candidatos válidos, mejor L/D hasta el momento) tras cada "
    "generación.",
    "Initialize the population as a tight cluster of small perturbations around the initial/preset design (plus the design itself, unperturbed) instead of SciPy's default uniform latin-hypercube coverage of the whole bounds space. Guarantees at least one known-valid, physically-balanced design is in generation 0, and lets the solver refine from there instead of having to rediscover CG/stability balance from scratch across the full 16-D space. Disable to fall back to the old full-space exploration (e.g. if you specifically want to explore far from the initial design).": "Inicializa la población como un grupo compacto de pequeñas perturbaciones alrededor del diseño inicial o "
    "del preajuste (más el propio diseño sin perturbar), en lugar del muestreo uniforme por hipercubo latino "
    "que SciPy hace por defecto sobre todo el espacio de límites. Garantiza que al menos un diseño válido y "
    "físicamente equilibrado esté en la generación 0 y permite al solver refinar desde ahí, en vez de tener "
    "que redescubrir el equilibrio de CG y estabilidad desde cero en un espacio de 16 dimensiones. Desactívalo "
    "para volver a la exploración del espacio completo.",
    "Size of the initial random perturbation around the initial design, as a fraction of each design variable's (upper - lower) bound range. Only used when seed_near_initial_design is enabled. Small values (e.g. 0.05) start with a tight, mostly-valid cluster; larger values explore more broadly from the start at the cost of more of the population starting off invalid.": "Tamaño de la perturbación aleatoria inicial alrededor del diseño de partida, como fracción del rango "
    "(superior - inferior) de cada variable de diseño. Solo se usa con seed_near_initial_design activado. "
    "Valores pequeños (p. ej. 0,05) parten de un grupo compacto y mayoritariamente válido; valores mayores "
    "exploran más desde el principio a costa de que más individuos empiecen siendo inválidos.",
    # --- Analysis fidelity --------------------------------------------------
    "Lowest angle of attack evaluated in the final high-fidelity polar sweep.": "Ángulo de ataque más bajo evaluado en el barrido final de alta fidelidad de la polar.",
    "Highest angle of attack evaluated in the final high-fidelity polar sweep.": "Ángulo de ataque más alto evaluado en el barrido final de alta fidelidad de la polar.",
    "How many angle-of-attack points to evaluate between the min/max alpha. More points = smoother drag polar, slower analysis. Part of the Fidelity preset.": "Cuántos puntos de ángulo de ataque se evalúan entre el alfa mínimo y el máximo. Más puntos = polar más "
    "suave pero análisis más lento. Forma parte del preajuste de fidelidad.",
    "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. Higher = finer mesh, slower. Part of the Fidelity preset.": "Multiplicador de la subdivisión de paneles en envergadura de cada superficie para el método de red de "
    "torbellinos. Más alto = malla más fina y más lenta. Forma parte del preajuste de fidelidad.",
    "Multiplier on each surface's built-in chordwise panel subdivision for the vortex-lattice solver. Higher = finer mesh, slower. Part of the Fidelity preset. Used by the fast in-loop estimate; the final reported analysis uses fine_chordwise_resolution instead.": "Multiplicador de la subdivisión de paneles en cuerda de cada superficie para el método de red de "
    "torbellinos. Más alto = malla más fina y más lenta. Forma parte del preajuste de fidelidad. Lo usa la "
    "estimación rápida dentro del bucle; el análisis final utiliza fine_chordwise_resolution.",
    "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, trimmed cruise point, neutral point) -- not the optimizer loop. Higher fidelity where speed doesn't matter.": "Resolución de paneles en envergadura empleada SOLO en el análisis final que se ejecuta una vez por "
    "ejecución (polar de resistencia, punto de crucero equilibrado, punto neutro), no en el bucle del "
    "optimizador. Mayor fidelidad donde la velocidad no importa.",
    "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical/cambered section needs ~8 chordwise panels for the VLM to resolve its camber line; at the coarse in-loop resolution the camber (and hence the zero-lift alpha) is under-captured, which inflates the reported cruise alpha by several degrees and under-predicts L/D by ~7%. Kept high here so the REPORTED cruise alpha (~1-4 deg, matching SUAVE) and L/D are physically accurate.": "Resolución de paneles en cuerda para el análisis final que se ejecuta una vez por ejecución. Un perfil "
    "supercrítico o con curvatura necesita ~8 paneles en cuerda para que el VLM resuelva su línea de "
    "curvatura; con la resolución gruesa del bucle la curvatura (y por tanto el alfa de sustentación nula) "
    "queda infracapturada, lo que infla el alfa de crucero reportado en varios grados y subestima el L/D en "
    "torno a un 7 %. Se mantiene alta aquí para que el alfa de crucero y el L/D REPORTADOS sean físicamente "
    "correctos.",
    "Lower of a two-point alpha pair used for a fast in-loop lift-slope/stability estimate (not the full sweep).": "El menor de un par de alfas usado para una estimación rápida de pendiente de sustentación y estabilidad "
    "dentro del bucle (no el barrido completo).",
    "Higher of the two-point fast-probe alpha pair.": "El mayor del par de alfas de sondeo rápido.",
    "Small horizontal-stabilizer incidence perturbation used to estimate dCL/di_h and dCm/di_h for the closed-form longitudinal trim solve (alpha, tail incidence) run inside the optimizer loop and the final full analysis. Smaller values are more locally linear but noisier; 0.5-2 deg is typical for a small-perturbation VLM probe. See methods.md Sec 9f.": "Pequeña perturbación de la incidencia del estabilizador horizontal usada para estimar dCL/di_h y dCm/di_h "
    "en la resolución analítica del equilibrado longitudinal (alfa e incidencia de cola) que se ejecuta dentro "
    "del bucle del optimizador y en el análisis final. Valores menores son más lineales localmente pero más "
    "ruidosos; 0,5-2° es lo habitual para un sondeo VLM de pequeña perturbación.",
    "Airspeed used for the autobalance neutral-point probe (finds the CG that gives the target static margin).": "Velocidad empleada en el sondeo del punto neutro del autoequilibrado (halla el CG que da el margen "
    "estático objetivo).",
    "Lower alpha of the two-point pair used to estimate the Cm-Cl slope during autobalance.": "Alfa inferior del par usado para estimar la pendiente Cm-Cl durante el autoequilibrado.",
    "Higher alpha of the two-point pair used to estimate the Cm-Cl slope during autobalance.": "Alfa superior del par usado para estimar la pendiente Cm-Cl durante el autoequilibrado.",
    "Ratio of the tail's local dynamic pressure to freestream (the tail sits in the wing wake / fuselage boundary layer). Standard range 0.85-0.95; lower it to make the tail less stabilising (NP moves forward).": "Relación entre la presión dinámica local en la cola y la de la corriente libre (la cola queda en la estela "
    "del ala y la capa límite del fuselaje). Rango habitual 0,85-0,95; reducirlo hace la cola menos "
    "estabilizadora (el punto neutro se adelanta).",
    "Whether to include the geometry-driven fuselage (Munk/Multhopp) destabilising contribution, which moves the neutral point forward.": "Si se incluye la contribución desestabilizadora del fuselaje derivada de su geometría (Munk/Multhopp), que "
    "adelanta el punto neutro.",
    "Lower CL bound of the window used to fit the parabolic drag polar (CD = CD0 + k*CL^2). Points outside the window are excluded so stall/pre-stall regions don't bias the fit.": "Límite inferior de CL de la ventana usada para ajustar la polar parabólica (CD = CD0 + k*CL²). Los puntos "
    "fuera de la ventana se excluyen para que las zonas de entrada en pérdida no sesguen el ajuste.",
    "Upper CL bound of the primary drag-polar fit window.": "Límite superior de CL de la ventana principal de ajuste de la polar.",
    "Wider fallback lower CL bound used when the primary fit window captures fewer than 3 points.": "Límite inferior de CL alternativo, más amplio, usado cuando la ventana principal captura menos de 3 puntos.",
    "Wider fallback upper CL bound used when the primary fit window captures fewer than 3 points.": "Límite superior de CL alternativo, más amplio, usado cuando la ventana principal captura menos de 3 puntos.",
    # --- Field performance --------------------------------------------------
    "Maximum lift coefficient achievable in the take-off flap/slat configuration. Drives take-off field length via the matching chart.": "Coeficiente de sustentación máximo alcanzable en configuración de despegue (flaps y slats). Determina la "
    "longitud de campo de despegue a través del diagrama de adaptación.",
    "Maximum lift coefficient achievable in the landing flap/slat configuration. Drives landing distance.": "Coeficiente de sustentación máximo alcanzable en configuración de aterrizaje. Determina la distancia de "
    "aterrizaje.",
    "Maximum lift coefficient in clean (flaps/slats up) configuration -- the true aerodynamic stall limit used for the V-n diagram's stall boundary, distinct from the flaps-down CLmax_TO/CLmax_L above.": "Coeficiente de sustentación máximo en configuración limpia (flaps y slats recogidos): el límite "
    "aerodinámico real de entrada en pérdida usado en el diagrama V-n, distinto de los CLmax de despegue y "
    "aterrizaje anteriores.",
    "Most negative (inverted-flight) lift coefficient in clean configuration -- the negative stall boundary on the V-n diagram.": "Coeficiente de sustentación más negativo (vuelo invertido) en configuración limpia: el límite negativo de "
    "entrada en pérdida del diagrama V-n.",
    "Fraction by which static sea-level thrust falls off during the take-off ground roll / initial climb, used in the matching-chart take-off constraint.": "Fracción en que decae el empuje estático a nivel del mar durante la carrera de despegue y el ascenso "
    "inicial, empleada en la restricción de despegue del diagrama de adaptación.",
    "FAR 25.121 minimum second-segment climb gradient with one engine inoperative (OEI). The matching chart auto-selects 0.024 (twin) / 0.027 (tri-jet) / 0.030 (quad) from the actual engine count; this value is only the fallback for any other engine count.": "Gradiente mínimo de ascenso en segundo segmento con un motor inoperativo (OEI) según FAR 25.121. El "
    "diagrama de adaptación selecciona automáticamente 0,024 (bimotor), 0,027 (trimotor) o 0,030 "
    "(cuatrimotor) según el número real de motores; este valor solo se usa como alternativa para otros casos.",
    "Empirical constant relating approach speed / wing loading to landing ground-roll + air distance.": "Constante empírica que relaciona la velocidad de aproximación y la carga alar con la carrera de "
    "aterrizaje más la distancia en el aire.",
    "Lift coefficient assumed in the take-off configuration when evaluating OEI second-segment climb L/D (Raymer Ch.17).": "Coeficiente de sustentación supuesto en configuración de despegue al evaluar el L/D del ascenso en "
    "segundo segmento con OEI (Raymer, cap. 17).",
    "Parasite-drag increment added to the clean CD0 for the flap/gear-down OEI second-segment climb configuration.": "Incremento de resistencia parásita añadido al CD0 limpio para la configuración de ascenso en segundo "
    "segmento con OEI, con flaps y tren desplegados.",
    "Lower wing-loading (W/S) axis limit on the matching chart plot. Widen for very light (GA) aircraft. (~204 kg/m^2)": "Límite inferior del eje de carga alar (W/S) en el diagrama de adaptación. Amplíalo para aviones muy "
    "ligeros de aviación general. (~204 kg/m²)",
    "Upper wing-loading (W/S) axis limit on the matching chart plot. Widen for very heavy (freighter) aircraft. (~1020 kg/m^2)": "Límite superior del eje de carga alar (W/S) en el diagrama de adaptación. Amplíalo para aviones muy "
    "pesados de carga. (~1020 kg/m²)",
    "BFL = bfl_factor x TODR (take-off distance required). Raymer Table 17.1: 1.15 for twin jets, ~1.18 for quads.": "BFL = bfl_factor x TODR (distancia de despegue requerida). Tabla 17.1 de Raymer: 1,15 para bimotores a "
    "reacción y ~1,18 para cuatrimotores.",
    "Minimum-control speed as a multiple of take-off stall speed. FAR 25.149 caps VMC at 1.13*VSR; that ceiling is used as the default.": "Velocidad mínima de control como múltiplo de la velocidad de entrada en pérdida en despegue. FAR 25.149 "
    "limita VMC a 1,13*VSR; ese techo se usa por defecto.",
    "FAR 25.107: rotation speed must be at least 1.05*VMC.": "FAR 25.107: la velocidad de rotación debe ser al menos 1,05*VMC.",
    "FAR 25.107: rotation speed must also be at least 1.10*VS_TO. VR = max(vr_vmc_factor*VMC, vr_vstall_factor*VS_TO).": "FAR 25.107: la velocidad de rotación debe ser además al menos 1,10*VS_TO. "
    "VR = máx(vr_vmc_factor*VMC, vr_vstall_factor*VS_TO).",
    "Take-off safety speed as a multiple of take-off stall speed (FAR 25.107). V2 = max(v2_vstall_factor*VS_TO, VR).": "Velocidad de seguridad al despegue como múltiplo de la velocidad de entrada en pérdida en despegue "
    "(FAR 25.107). V2 = máx(v2_vstall_factor*VS_TO, VR).",
    "Decision speed as a fraction of rotation speed. On a dry balanced field V1 sits just below VR (~0.95-0.98); the earlier 0.90 default put V1 unrealistically far below VR (e.g. ~152 kt against a real 787-9 V1 of 160-165 kt).": "Velocidad de decisión como fracción de la velocidad de rotación. En campo equilibrado y seco V1 queda algo "
    "por debajo de VR (~0,95-0,98); el valor anterior de 0,90 situaba V1 irrealmente lejos de VR (p. ej. "
    "~152 kt frente a los 160-165 kt reales de un 787-9).",
    "Approach speed as a multiple of landing stall speed (FAR 25.125).": "Velocidad de aproximación como múltiplo de la velocidad de entrada en pérdida en aterrizaje (FAR 25.125).",
    "Touchdown speed as a multiple of landing stall speed.": "Velocidad de toma de contacto como múltiplo de la velocidad de entrada en pérdida en aterrizaje.",
    "Number of wing-loading points swept when drawing the matching-chart constraint curves (Results -> Matching Chart). Purely a plotting resolution knob -- higher gives smoother curves at extra compute cost. Not part of the Performance preset (it isn't a physical assumption).": "Número de puntos de carga alar recorridos al dibujar las curvas de restricción del diagrama de adaptación. "
    "Es puramente un ajuste de resolución del gráfico: más alto da curvas más suaves a costa de más cálculo. No "
    "forma parte del preajuste de actuaciones, ya que no es una hipótesis física.",
    # --- Mass model ---------------------------------------------------------
    "Fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula (everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78.": "Fracción del MTOW tratada como masa 'suspendida' en la fórmula estructural del ala de Torenbeek (todo lo "
    "que la estructura alar debe soportar aparte de sí misma). Transporte comercial típico: 0,70-0,78.",
    "Design airspeed with flaps extended, fed into the Torenbeek wing-mass formula.": "Velocidad de diseño con flaps extendidos, empleada en la fórmula de masa alar de Torenbeek.",
    "Maximum flap deflection angle, fed into the Torenbeek wing-mass formula.": "Ángulo máximo de deflexión de flap, empleado en la fórmula de masa alar de Torenbeek.",
    "Landing gear mass as a fraction of MTOW. Raymer Table 15.2: ~4% for commercial jet transports.": "Masa del tren de aterrizaje como fracción del MTOW. Tabla 15.2 de Raymer: ~4 % para reactores comerciales "
    "de transporte.",
    "Dry engine mass is estimated as thrust / (this factor * g). Historical engine thrust-to-weight ratios are ~5-7, so this factor is typically ~6.": "La masa en seco del motor se estima como empuje / (este factor * g). Las relaciones empuje/peso históricas "
    "de los motores son de ~5-7, por lo que este factor suele valer ~6.",
    "Multiplier on dry engine mass accounting for pylon, cowling, fire suppression and other installed accessories.": "Multiplicador sobre la masa en seco del motor que contabiliza el pilón, la carena, la extinción de "
    "incendios y demás accesorios instalados.",
    "Fallback propulsion mass as a fraction of MTOW, used only if the selected engine isn't found in the database.": "Masa de propulsión alternativa como fracción del MTOW, empleada solo si el motor seleccionado no se "
    "encuentra en la base de datos.",
    "Avionics, electrical, ECS, APU, etc. as a fraction of MTOW. Raymer Table 15.2: 9-13% for commercial transports.": "Aviónica, sistema eléctrico, aire acondicionado, APU, etc., como fracción del MTOW. Tabla 15.2 de Raymer: "
    "9-13 % para transportes comerciales.",
    "Passenger seats, galleys, lavatories, insulation, crew, paint, and operational empty items as a fraction of MTOW. Typically 10-14% for passenger transports.": "Asientos, offices, aseos, aislamiento, tripulación, pintura y demás partidas del vacío operativo como "
    "fracción del MTOW. Habitualmente 10-14 % en transportes de pasajeros.",
    "How much payload mass occupies one metre of cabin length. Used only to derive the payload/systems CG position (the occupied cabin length), not the payload mass itself -- so stretching the fuselage beyond what the payload needs doesn't shift the CG aft 'for free'.": "Cuánta masa de carga de pago ocupa un metro de longitud de cabina. Solo se usa para deducir la posición "
    "del CG de la carga y los sistemas (la longitud de cabina ocupada), no la masa de carga en sí, de modo que "
    "alargar el fuselaje más de lo que la carga necesita no desplaza el CG hacia atrás 'gratis'.",
    "Jet-A/Jet-A1 density at 15C (~804 kg/m^3). Converts wing tank volume to a fuel-mass capacity for the payload-range diagram and the wing fuel-volume check.": "Densidad del Jet-A/Jet-A1 a 15 °C (~804 kg/m³). Convierte el volumen de los depósitos del ala en capacidad "
    "de masa de combustible para el diagrama carga-alcance y la comprobación de volumen de combustible.",
    "Fraction of the wing's geometric (Torenbeek) fuel volume that's actually usable tank capacity, after structure, ribs, systems and unusable-fuel allowance. Typical preliminary-design value: 0.85-0.95.": "Fracción del volumen geométrico de combustible del ala (Torenbeek) que resulta realmente utilizable, una "
    "vez descontados estructura, costillas, sistemas y combustible no utilizable. Valor típico en diseño "
    "preliminar: 0,85-0,95.",
    # --- Landing gear -------------------------------------------------------
    "Nose landing gear longitudinal position, as a fraction of total fuselage length from the nose.": "Posición longitudinal del tren de morro, como fracción de la longitud total del fuselaje desde el morro.",
    "Main landing gear longitudinal position, as a fraction of the mean aerodynamic chord aft of the MAC leading edge.": "Posición longitudinal del tren principal, como fracción de la cuerda media aerodinámica por detrás de su "
    "borde de ataque.",
    "Maximum fraction of total aircraft weight the nose gear is rated to carry -- sets the 'NLG Max Strength' CG-envelope boundary.": "Fracción máxima del peso total de la aeronave que el tren de morro está calificado para soportar: define "
    "el límite de resistencia máxima del tren de morro en la envolvente de CG.",
    "Maximum fraction of total aircraft weight the main gear is rated to carry -- sets the 'MLG Max Strength' CG-envelope boundary.": "Fracción máxima del peso total de la aeronave que el tren principal está calificado para soportar: define "
    "el límite de resistencia máxima del tren principal en la envolvente de CG.",
    "Minimum fraction of weight that must be on the nose gear for adequate steering authority -- sets the 'Min Nose Load' CG-envelope boundary (the aft-most safe CG at each weight).": "Fracción mínima del peso que debe recaer sobre el tren de morro para tener autoridad de gobierno "
    "suficiente: define el límite de carga mínima en morro de la envolvente de CG (el CG más retrasado seguro "
    "a cada peso).",
    "Maximum Landing Weight (MLW) as a fraction of MTOW, shown as a reference line on the CG envelope.": "Peso máximo de aterrizaje (MLW) como fracción del MTOW, mostrado como línea de referencia en la "
    "envolvente de CG.",
    "Margin applied to the static reaction load when selecting/verifying tire count -- real gear is sized so the rated tire load is never fully consumed by static load alone, leaving margin for dynamic (braking, turning, rough-field) loads. Raymer: ~1.07 typical for a preliminary sizing pass.": "Margen aplicado a la reacción estática al seleccionar o verificar el número de neumáticos: el tren real se "
    "dimensiona de modo que la carga nominal del neumático nunca se consuma solo con la carga estática, "
    "dejando margen para cargas dinámicas (frenado, viraje, pista en mal estado). Raymer: ~1,07 típico en un "
    "dimensionado preliminar.",
    "Wheels on the nose gear strut. 0 = auto: 1 for light aircraft, 2 (the near-universal choice for CS-25/FAR-25 transports) once MTOW exceeds nlg_dual_wheel_mtow_kg.": "Ruedas en la pata del tren de morro. 0 = automático: 1 para aviones ligeros y 2 (la opción casi universal "
    "en transportes CS-25/FAR-25) cuando el MTOW supera nlg_dual_wheel_mtow_kg.",
    "Auto-sizing switches from a single to a dual (twin) nose wheel above this MTOW -- below it, transport-category aircraft still commonly fly single nose wheels.": "El dimensionado automático pasa de rueda de morro simple a doble por encima de este MTOW; por debajo, los "
    "aviones de transporte todavía suelen llevar rueda de morro simple.",
    "Number of main-gear legs (each with its own wheel bogie), left+right combined. 0 = auto: 2 (one per side) below mlg_body_gear_mtow_kg, 4 (adds centreline body gear, e.g. A380/747-class) above it -- real widebodies above roughly 300 t add body gear because a two-leg bogie would need an impractically large tire count/track width to carry the load within tire-pressure limits.": "Número de patas del tren principal (cada una con su propio bogie), sumando izquierda y derecha. "
    "0 = automático: 2 (una por lado) por debajo de mlg_body_gear_mtow_kg y 4 (añade tren de fuselaje central, "
    "tipo A380/747) por encima. Los fuselajes anchos reales de más de unas 300 t añaden tren de fuselaje "
    "porque un bogie de dos patas necesitaría un número de neumáticos o una vía impracticables para soportar "
    "la carga dentro de los límites de presión.",
    "Auto-sizing adds two centreline body-gear legs (4 main legs total) above this MTOW.": "El dimensionado automático añade dos patas de tren de fuselaje central (4 patas principales en total) por "
    "encima de este MTOW.",
    "0 = auto: the smallest of {2, 4, 6} standard bogie sizes whose rated capacity (tire_safety_factor-derated) covers this strut's static reaction load at the aft CG limit.": "0 = automático: el menor de los tamaños estándar de bogie {2, 4, 6} cuya capacidad nominal (reducida por "
    "tire_safety_factor) cubre la reacción estática de esta pata en el límite trasero de CG.",
    "Main-gear lateral track width, as a multiple of fuselage diameter. Real transports with wing-root-mounted main gear run track/diameter ~1.75-2.0 (777-300ER 2.03, 787-9 1.90, A340-300 1.91, A380-800 2.00, A320-200 1.92, DC-10-30 1.77) -- 1.85 is the fleet-average calibration. An earlier default (1.15) understated real track width by roughly a factor of 1.6, which fed directly into the lateral-turnover check (physics.landing_gear) reading artificially safe.": "Anchura de vía del tren principal, como múltiplo del diámetro del fuselaje. Los transportes reales con "
    "tren principal anclado en la raíz alar tienen vía/diámetro ~1,75-2,0 (777-300ER 2,03; 787-9 1,90; "
    "A340-300 1,91; A380-800 2,00; A320-200 1,92; DC-10-30 1,77); 1,85 es la calibración media de la flota. Un "
    "valor anterior (1,15) subestimaba la vía real en un factor de aproximadamente 1,6, lo que hacía que la "
    "comprobación de vuelco lateral resultara artificialmente segura.",
    "Which reference tire (see physics.landing_gear.TIRE_DATABASE) to size with -- 'auto' picks the smallest class whose rated load, combined with a realistic wheel count (<=6/strut), covers the aircraft's static gear loads. Options: auto, light, narrowbody, widebody, heavy.": "Qué neumático de referencia usar para el dimensionado: 'auto' elige la clase más pequeña cuya carga "
    "nominal, combinada con un número realista de ruedas (<=6 por pata), cubre las cargas estáticas del tren. "
    "Opciones: auto, light, narrowbody, widebody, heavy.",
    "Landing-gear strut/piston material, shown on the planform diagram and in the design report. 'auto' selects by MTOW class (see physics.landing_gear.STRUT_MATERIALS): high-strength steel (300M-class) for larger transports, an aluminium/steel combination for light aircraft. Informational/labelling only -- this preliminary-design tool does not run a structural (FEA) stress analysis of the strut itself.": "Material de la pata o vástago del tren, mostrado en el diagrama en planta y en el informe de diseño. "
    "'auto' lo elige según la clase de MTOW: acero de alta resistencia (tipo 300M) para transportes grandes y "
    "una combinación de aluminio y acero para aviones ligeros. Solo informativo: esta herramienta de diseño "
    "preliminar no realiza un análisis estructural por elementos finitos de la propia pata.",
    "Lateral tip-over (overturn) criterion, Raymer Ch.11 / Currey convention: the angle from the vertical whose tangent is CG height over the CG's perpendicular distance to the nose-gear-to-main-gear ground line must not exceed this (evaluated at the forward CG limit, the worst case), or the aircraft risks tipping over in a tight turn. 63 deg is the standard transport-category limit; a higher CG, narrower track, or more forward CG all push the angle up toward it.": "Criterio de vuelco lateral (convenio de Raymer cap. 11 / Currey): el ángulo respecto a la vertical cuya "
    "tangente es la altura del CG dividida por su distancia perpendicular a la línea de tierra entre el tren de "
    "morro y el principal no debe superar este valor (evaluado en el límite delantero de CG, el caso más "
    "desfavorable), o la aeronave corre riesgo de volcar en un viraje cerrado. 63° es el límite estándar en "
    "categoría transporte; un CG más alto, una vía más estrecha o un CG más adelantado acercan el ángulo a ese "
    "límite.",
    # --- MSES ---------------------------------------------------------------
    "Run an MSES 2-D polar sweep on the optimized design's root airfoil section as part of a normal Run, populating the Model Comparison tab. On by default. Set on Setup > External Tools.": "Ejecuta un barrido de polar 2-D con MSES sobre el perfil de raíz del diseño optimizado como parte de una "
    "ejecución normal, rellenando la pestaña Comparación de modelos. Activado por defecto. Se configura en "
    "Configuración > Herramientas externas.",
    "Path (repo-root-relative or absolute) to the folder containing mset.exe/mses.exe/mplot.exe. Set on Setup > External Tools.": "Ruta (relativa a la raíz del repositorio o absoluta) a la carpeta que contiene mset.exe, mses.exe y "
    "mplot.exe. Se configura en Configuración > Herramientas externas.",
    "Max time allowed for one mesh-generation (mset) call before it's killed.": "Tiempo máximo permitido para una llamada de generación de malla (mset) antes de abortarla.",
    "Max time allowed for one flow-solve (mses) call before it's killed.": "Tiempo máximo permitido para una llamada de resolución del flujo (mses) antes de abortarla.",
    "Newton iteration cap per angle of attack -- MSES reports non-convergence rather than looping forever, but a hard cap keeps a single stubborn point from stalling the whole sweep.": "Límite de iteraciones de Newton por ángulo de ataque: MSES informa de no convergencia en lugar de iterar "
    "indefinidamente, pero un tope estricto evita que un único punto difícil bloquee todo el barrido.",
    "e^N transition-prediction critical amplification factor. 9.0 is the standard sea-level-cruise default (Drela); lower values (e.g. 4-5) predict earlier transition, appropriate for a high-turbulence/rough-surface environment.": "Factor crítico de amplificación e^N para la predicción de la transición. 9,0 es el valor estándar en "
    "crucero a nivel del mar (Drela); valores menores (p. ej. 4-5) predicen una transición más temprana, "
    "apropiada en entornos de alta turbulencia o superficies rugosas.",
    "Force transition at this upper-surface x/c instead of letting MSES predict it. 1.0 = free (natural) transition, the realistic default for a clean cruise wing.": "Fuerza la transición en esta posición x/c del extradós en lugar de dejar que MSES la prediga. 1,0 = "
    "transición libre (natural), lo realista para un ala limpia en crucero.",
    "Force transition at this lower-surface x/c. 1.0 = free (natural) transition.": "Fuerza la transición en esta posición x/c del intradós. 1,0 = transición libre (natural).",
    "The MSES polar sweeps [trim_alpha - this, trim_alpha + this] so the comparison brackets the actual cruise operating point, not an arbitrary fixed range.": "La polar de MSES barre [alfa_equilibrado - este valor, alfa_equilibrado + este valor], de modo que la "
    "comparación abarca el punto de operación real en crucero y no un rango fijo arbitrario.",
    "Number of alpha points in the MSES polar sweep. Kept small relative to AeroSandbox's own VLM sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve (~1-2s) rather than a linear-algebra VLM solve.": "Número de puntos de alfa en el barrido de polar de MSES. Se mantiene pequeño frente al barrido VLM propio "
    "de AeroSandbox, ya que cada punto de MSES es una resolución viscosa y compresible real (~1-2 s) y no un "
    "cálculo VLM de álgebra lineal.",
    "Number of surface panel nodes MSET generates for the airfoil mesh.": "Número de nodos de panel de superficie que MSET genera para la malla del perfil.",
    "Streamwise grid stretching parameter for the MSET mesh -- larger values cluster more points near the airfoil.": "Parámetro de estiramiento de la malla en la dirección de la corriente para MSET: valores mayores "
    "concentran más puntos cerca del perfil.",
    # --- Control surfaces ---------------------------------------------------
    "Leading-edge slat chord as a fraction of local wing chord.": "Cuerda del slat de borde de ataque como fracción de la cuerda local del ala.",
    "Inboard end of the slat run, as a fraction of wing semi-span from the root.": "Extremo interior del tramo de slat, como fracción de la semienvergadura desde la raíz.",
    "Outboard end of the slat run, as a fraction of wing semi-span from the root.": "Extremo exterior del tramo de slat, como fracción de la semienvergadura desde la raíz.",
    "Trailing-edge flap chord as a fraction of local wing chord.": "Cuerda del flap de borde de salida como fracción de la cuerda local del ala.",
    "Inboard end of the flap run (just outside the fuselage), as a fraction of wing semi-span.": "Extremo interior del tramo de flap (justo fuera del fuselaje), como fracción de la semienvergadura.",
    "Outboard end of the flap run, as a fraction of wing semi-span.": "Extremo exterior del tramo de flap, como fracción de la semienvergadura.",
    "Aileron chord as a fraction of local wing chord.": "Cuerda del alerón como fracción de la cuerda local del ala.",
    "Inboard end of the aileron run, as a fraction of wing semi-span.": "Extremo interior del tramo de alerón, como fracción de la semienvergadura.",
    "Outboard end of the aileron run, as a fraction of wing semi-span.": "Extremo exterior del tramo de alerón, como fracción de la semienvergadura.",
    "Spoiler/speedbrake chord as a fraction of local wing chord (ahead of the flaps).": "Cuerda del spoiler o aerofreno como fracción de la cuerda local del ala (por delante de los flaps).",
    "Inboard end of the spoiler run, as a fraction of wing semi-span.": "Extremo interior del tramo de spoiler, como fracción de la semienvergadura.",
    "Outboard end of the spoiler run (typically spanning the flap run), as a fraction of wing semi-span.": "Extremo exterior del tramo de spoiler (normalmente abarcando el tramo de flap), como fracción de la "
    "semienvergadura.",
    "Elevator chord as a fraction of local horizontal-stabilizer chord.": "Cuerda del timón de profundidad como fracción de la cuerda local del estabilizador horizontal.",
    "Inboard end of the elevator run, as a fraction of h-stab semi-span.": "Extremo interior del tramo de timón de profundidad, como fracción de la semienvergadura del estabilizador "
    "horizontal.",
    "Outboard end of the elevator run, as a fraction of h-stab semi-span.": "Extremo exterior del tramo de timón de profundidad, como fracción de la semienvergadura del estabilizador "
    "horizontal.",
    "Rudder chord as a fraction of local vertical-stabilizer chord.": "Cuerda del timón de dirección como fracción de la cuerda local del estabilizador vertical.",
    "Root-ward end of the rudder run, as a fraction of v-stab span.": "Extremo hacia la raíz del tramo de timón de dirección, como fracción de la envergadura del estabilizador "
    "vertical.",
    "Tip-ward end of the rudder run, as a fraction of v-stab span.": "Extremo hacia la punta del tramo de timón de dirección, como fracción de la envergadura del estabilizador "
    "vertical.",
    # --- Propulsion cycle ---------------------------------------------------
    "Total-pressure recovery through the inlet (ram + duct losses). Matches SUAVE's inlet_nozzle.pressure_ratio.": "Recuperación de presión total a través de la toma (pérdidas de impacto y de conducto). Coincide con "
    "inlet_nozzle.pressure_ratio de SUAVE.",
    "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). Matches SUAVE's fixed LPC split.": "Relación de presiones fija del compresor de baja presión (booster); el compresor de alta aporta el resto "
    "de la relación global del núcleo (HPC = OPR / este valor). Coincide con el reparto fijo de LPC de SUAVE.",
    "Total-pressure loss fraction across the combustor.": "Fracción de pérdida de presión total en la cámara de combustión.",
    "Shaft power-transmission efficiency for both spools (HPT-HPC, LPT-LPC+fan).": "Rendimiento de transmisión de potencia por eje para ambos ejes (HPT-HPC y LPT-LPC+fan).",
    "Polytropic expansion efficiency of the core exhaust nozzle.": "Rendimiento politrópico de expansión de la tobera de escape del núcleo.",
    "Polytropic expansion efficiency of the fan (bypass) nozzle.": "Rendimiento politrópico de expansión de la tobera del fan (flujo derivado).",
    "Lower heating value of Jet-A/Jet-A1 fuel.": "Poder calorífico inferior del combustible Jet-A/Jet-A1.",
    "Air-standard specific heat used for the inlet/fan/compressor (unburned-air) stations.": "Calor específico del aire estándar empleado en las estaciones de toma, fan y compresor (aire sin quemar).",
    "Combustion-gas specific heat used for the combustor/turbine/core-nozzle stations.": "Calor específico de los gases de combustión empleado en las estaciones de cámara, turbina y tobera del "
    "núcleo.",
    "Used only to sanity-check the design mass flow implied by anchoring the cycle to the engine's rated static thrust -- a conceptual-design-level assumption, not a real corrected-flow schedule.": "Solo se emplea para contrastar el gasto másico de diseño que resulta de anclar el ciclo al empuje "
    "estático nominal del motor: es una hipótesis de nivel conceptual, no una ley real de gasto corregido.",
    # --- Structures ---------------------------------------------------------
    "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN .bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of a normal Run, populating the Structural Analysis Results tab. Does not affect the mass model, CG, or optimizer -- purely a downstream analysis, like MSES/Propulsion Analysis.": "Dimensiona un cajón alar genérico (revestimiento, largueros y costillas) para el ala principal del diseño "
    "optimizado, escribe archivos .bdf de NASTRAN y calcula deformaciones, tensiones y frecuencias teóricas "
    "(sin NASTRAN) como parte de una ejecución normal, rellenando la pestaña de análisis estructural. No afecta "
    "al modelo de masas, al CG ni al optimizador: es un análisis posterior, como MSES o el análisis de "
    "propulsión.",
    "Chordwise position of each spar, as a fraction of local chord (0=leading edge, 1=trailing edge). One entry per spar, any order (sorted automatically). E.g. (0.25, 0.70) is a classic front/rear 2-spar box; add a third entry for a mid-spar.": "Posición en cuerda de cada larguero, como fracción de la cuerda local (0 = borde de ataque, 1 = borde de "
    "salida). Una entrada por larguero, en cualquier orden (se ordenan automáticamente). Por ejemplo, "
    "(0,25, 0,70) es el clásico cajón de dos largueros delantero y trasero; añade una tercera entrada para un "
    "larguero intermedio.",
    "Which ribs get trailing-edge panels (rear-spar to TE), preventing TE buckling: 'all', 'none', 'alternate', 'inboard' (only inboard of the wing break), 'outboard', or 'step_N' (every Nth rib).": "Qué costillas llevan paneles de borde de salida (del larguero trasero al borde de salida) para evitar su "
    "pandeo: 'all', 'none', 'alternate', 'inboard' (solo por dentro del quiebro alar), 'outboard' o 'step_N' "
    "(una de cada N costillas).",
    "Adds an optional third spar running only from the root to the wing break/kink station, at center_spar_chord_fraction of local chord -- the partial-span reinforcement spar common on widebody wings (extra bending/shear capacity where root load is highest, without the mass of running it all the way to the tip). Off by default (classic 2-spar box). This spar carries no load and contributes no mass outboard of the kink -- it simply doesn't exist there.": "Añade un tercer larguero opcional que va solo de la raíz a la estación del quiebro alar, situado en "
    "center_spar_chord_fraction de la cuerda local: el larguero de refuerzo de envergadura parcial habitual en "
    "alas de fuselaje ancho (más capacidad a flexión y cortadura donde la carga en raíz es mayor, sin la masa "
    "de prolongarlo hasta la punta). Desactivado por defecto (cajón clásico de dos largueros). Este larguero no "
    "soporta carga ni aporta masa por fuera del quiebro: simplemente no existe ahí.",
    "Chordwise position of the optional center spar (see center_spar_enabled), as a fraction of local chord. Only used when center_spar_enabled is True.": "Posición en cuerda del larguero central opcional, como fracción de la cuerda local. Solo se usa con "
    "center_spar_enabled activado.",
    "Material name from the built-in structural material database, used for the wing skin panels.": "Nombre del material, de la base de datos estructural incorporada, empleado en los paneles del "
    "revestimiento alar.",
    "Material for the spar shear webs.": "Material de las almas a cortadura de los largueros.",
    "Material for the spar caps (the primary bending-load-carrying members).": "Material de los cordones de los largueros (los elementos que soportan principalmente la flexión).",
    "Material for the rib webs.": "Material de las almas de las costillas.",
    "Extra margin multiplied onto the design loads on top of the CS-25 ultimate load factors already used (DesignRequirements.ultimate_load_factor / limit_load_factor_neg). 1.0 = no extra margin beyond CS-25 ultimate.": "Margen adicional que multiplica las cargas de diseño por encima de los factores de carga últimos de CS-25 "
    "ya empleados. 1,0 = sin margen adicional sobre el último de CS-25.",
    "Wing skin thickness -- this is the value actually used (no shear-flow/buckling upsizing is modeled, see structural_sizing.py), so treat it as a practical starting assumption for this class of aircraft, not a bare absolute-minimum gauge. 6mm matches the reference sizing scripts' own baseline for a large long-range wing; a much thinner value (e.g. 2mm) understates real skin panel buckling resistance and inflates the auto-derived rib count substantially, since rib spacing scales with sqrt(t_skin).": "Espesor del revestimiento alar: es el valor que se usa realmente (no se modela un redimensionado por flujo "
    "cortante ni por pandeo), así que tómalo como una hipótesis de partida práctica para esta clase de avión y "
    "no como un espesor mínimo absoluto. 6 mm coincide con la referencia de los guiones de dimensionado para un "
    "ala grande de largo alcance; un valor mucho menor (p. ej. 2 mm) subestima la resistencia real al pandeo "
    "de los paneles e infla notablemente el número de costillas deducido automáticamente, ya que su separación "
    "escala con la raíz del espesor.",
    "Spanwise fraction below which spar caps keep their full root section (bending moment is highest inboard, so locking the section here preserves most of the tip-deflection stiffness). Above this station, caps taper linearly down to cap_taper_tip_fraction at the tip.": "Fracción de envergadura por debajo de la cual los cordones conservan su sección completa de raíz (el "
    "momento flector es máximo en la zona interior, así que fijar la sección aquí preserva la mayor parte de la "
    "rigidez frente a la flecha en punta). Por encima de esa estación, los cordones se estrechan linealmente "
    "hasta cap_taper_tip_fraction en la punta.",
    "Fraction of the locked-section cap flange width/thickness remaining at the wingtip.": "Fracción de la anchura y el espesor del ala del cordón (respecto a la sección fijada) que permanece en la "
    "punta del ala.",
    "Empirical panel-buckling coefficient (c) in the Euler skin-panel critical stress formula used to auto-derive rib spacing/count -- higher allows wider rib spacing for the same skin thickness.": "Coeficiente empírico de pandeo de panel (c) en la fórmula de tensión crítica de Euler empleada para "
    "deducir automáticamente la separación y el número de costillas: valores mayores permiten mayor separación "
    "para el mismo espesor de revestimiento.",
    "Effective radius of gyration of a skin panel stiffened by a stringer, used by the same rib-spacing buckling formula.": "Radio de giro efectivo de un panel de revestimiento rigidizado por un larguerillo, empleado por la misma "
    "fórmula de pandeo para la separación de costillas.",
    "Force an exact number of ribs instead of the auto-derived panel-buckling spacing. Leave blank to let the app determine rib count/spacing.": "Fuerza un número exacto de costillas en lugar de la separación deducida automáticamente por pandeo. Déjalo "
    "en blanco para que la aplicación determine el número y la separación.",
    "Number of spanwise points used for load/moment/deflection integration (sizing and the analytical deformation/stress solver). Higher = smoother curves, slower.": "Número de puntos en envergadura empleados para integrar cargas, momentos y flechas (dimensionado y "
    "resolución analítica de deformaciones y tensiones). Más alto = curvas más suaves pero más lento.",
    "Number of chordwise points sampled per rib cross-section in the FEM mesh.": "Número de puntos en cuerda muestreados por sección de costilla en la malla de elementos finitos.",
    "Path (repo-root-relative or absolute) to nastran.exe. Leave blank to only generate .bdf files and use the theoretical (analytical) deformation/stress/frequency estimates -- no NASTRAN install is required for that path. Set on Setup > External Tools.": "Ruta (relativa a la raíz del repositorio o absoluta) a nastran.exe. Déjalo en blanco para generar "
    "únicamente archivos .bdf y usar las estimaciones teóricas (analíticas) de deformación, tensión y "
    "frecuencia: esa vía no requiere ninguna instalación de NASTRAN. Se configura en Configuración > "
    "Herramientas externas.",
    "Actually invoke nastran_exe_path as a subprocess on the generated .bdf files. On by default: when the executable isn't configured/found, the app still writes valid .bdf files and shows the theoretical (analytical) results only, so leaving this on is always safe. Set on Setup > External Tools.": "Invoca realmente nastran_exe_path como subproceso sobre los archivos .bdf generados. Activado por defecto: "
    "cuando el ejecutable no está configurado o no se encuentra, la aplicación sigue escribiendo archivos .bdf "
    "válidos y muestra solo los resultados teóricos, así que dejarlo activado es siempre seguro. Se configura "
    "en Configuración > Herramientas externas.",
    "Pull-up / push-down / 1g-level static load cases -> deformation and stress.": "Casos de carga estática de recurso, picado y vuelo nivelado a 1 g, con sus deformaciones y tensiones.",
    "Natural frequencies and mode shapes.": "Frecuencias naturales y formas modales.",
    "Modal frequency response to a harmonic engine-mounted excitation force.": "Respuesta modal en frecuencia a una fuerza de excitación armónica aplicada en el anclaje del motor.",
    "Modal random-vibration response (PSD) to a white-noise engine-mounted excitation.": "Respuesta modal a vibración aleatoria (PSD) frente a una excitación de ruido blanco en el anclaje del "
    "motor.",
    "Max time allowed for one NASTRAN solution (SOL 101/103/111 each run separately) before it's killed.": "Tiempo máximo permitido para una solución de NASTRAN (SOL 101, 103 y 111 se ejecutan por separado) antes "
    "de abortarla.",
    "Max structural modes for SOL 103's EIGRL and the analytical Rayleigh-quotient estimate.": "Número máximo de modos estructurales para la tarjeta EIGRL de SOL 103 y para la estimación analítica por "
    "el cociente de Rayleigh.",
    "Structural damping assumed for the sine/random vibration response (2% is a common metallic-airframe assumption).": "Amortiguamiento estructural supuesto para la respuesta a vibración senoidal y aleatoria (un 2 % es una "
    "hipótesis habitual en células metálicas).",
    "Flat white-noise acceleration power spectral density applied at the excitation point for the random-vibration case.": "Densidad espectral de potencia de aceleración de ruido blanco plano aplicada en el punto de excitación "
    "para el caso de vibración aleatoria.",
    "Path (repo-root-relative or absolute) to patran.exe. Leave blank to skip -- no Patran install is required for anything else in Structural Analysis. Set on Setup > External Tools.": "Ruta (relativa a la raíz del repositorio o absoluta) a patran.exe. Déjalo en blanco para omitirlo: ninguna "
    "otra parte del análisis estructural requiere una instalación de Patran. Se configura en Configuración > "
    "Herramientas externas.",
    "After a successful NASTRAN SOL 101 static solve, batch-replay a Patran session per load case to export a deformation-plot PNG (same view MSC Patran's own interactive GUI shows). On by default; needs a real licensed Patran install and launches it as a subprocess per load case (a few seconds each) -- has no effect when patran_exe_path isn't configured. Requires run_nastran and run_sol_static to both be on. Set on Setup > External Tools.": "Tras una resolución estática SOL 101 correcta en NASTRAN, reproduce por lotes una sesión de Patran por "
    "cada caso de carga para exportar un PNG del gráfico de deformación (la misma vista que muestra la propia "
    "interfaz de MSC Patran). Activado por defecto; requiere una instalación con licencia real de Patran y la "
    "lanza como subproceso por cada caso de carga (unos segundos cada uno). No tiene efecto si patran_exe_path "
    "no está configurado. Requiere que run_nastran y run_sol_static estén activados. Se configura en "
    "Configuración > Herramientas externas.",
}

# --- Figure titles, axis labels and legend entries -------------------------
# Applied at the render boundary (sidecar/routes_figures._translate_figure), so
# the figure factories keep writing plain English. Symbols, units and
# coefficient names an engineer reads identically in both languages (CL, CD,
# Mach, TAS, EAS, x/c, MS, BFL, TODR, LDR, ASD, TSFC, SFC, EI, OPR, BPR, L/D)
# are deliberately left untranslated; only prose is.
_FIGURES = {
    # Axis labels
    "% MAC from LEMAC": "% MAC desde el LEMAC",
    "Aircraft Center of Gravity (% MAC)": "Centro de gravedad de la aeronave (% MAC)",
    "Airspeed (kt)": "Velocidad (kt)",
    "Altitude (ft)": "Altitud (ft)",
    "Altitude [km]": "Altitud [km]",
    "Angle of attack (deg)": "Ángulo de ataque (grados)",
    "Bending moment M [MN.m]": "Momento flector M [MN·m]",
    "Bending stiffness EI [GN.m^2]": "Rigidez a flexión EI [GN·m²]",
    "Bypass ratio  BPR  [-]": "Relación de derivación  BPR  [-]",
    "Chordwise position X [m]": "Posición en cuerda X [m]",
    "Deflection δ [m]": "Flecha δ [m]",
    "Distance [m]": "Distancia [m]",
    "Drag (kN)": "Resistencia (kN)",
    "Efficiency  [-]": "Rendimiento  [-]",
    "Equivalent airspeed (kt)": "Velocidad equivalente (kt)",
    "Frequency [Hz]": "Frecuencia [Hz]",
    "Fuel mass (t)": "Masa de combustible (t)",
    "Height Z (m)": "Altura Z (m)",
    "Imaginary part (1/s) — frequency": "Parte imaginaria (1/s) — frecuencia",
    "Latitude [deg]": "Latitud [grados]",
    "Lift (kN)": "Sustentación (kN)",
    "Lift per unit span L' [N/m]": "Sustentación por unidad de envergadura L' [N/m]",
    "Load factor n": "Factor de carga n",
    "Longitude [deg]": "Longitud [grados]",
    "Longitudinal X (m)": "Longitudinal X (m)",
    "Margin of safety [-]": "Margen de seguridad [-]",
    "Mass [kg]": "Masa [kg]",
    "Normalized mode shape": "Forma modal normalizada",
    "Payload (t)": "Carga de pago (t)",
    "Pitch angle (deg)": "Ángulo de cabeceo (grados)",
    "RMS displacement [m]": "Desplazamiento RMS [m]",
    "Range (nm)": "Alcance (nm)",
    "Real part (1/s) — damping": "Parte real (1/s) — amortiguamiento",
    "Spanwise position Y [m]": "Posición en envergadura Y [m]",
    "Spanwise station y [m]": "Estación en envergadura y [m]",
    "Span Y (m)": "Envergadura Y (m)",
    "Stagnation temperature [K]": "Temperatura de remanso [K]",
    "Thrust (kN)": "Empuje (kN)",
    "Thrust-to-weight  T0/W0  [-]": "Relación empuje/peso  T0/W0  [-]",
    "Total mass (t)": "Masa total (t)",
    "True airspeed (kt)": "Velocidad verdadera (kt)",
    "Weight (tonnes)": "Peso (toneladas)",
    "Wing fuel-tank capacity (kg)": "Capacidad de los depósitos del ala (kg)",
    "Wing loading  W/S  [kg/m2]": "Carga alar  W/S  [kg/m²]",
    "X [m] (fuselage station)": "X [m] (estación del fuselaje)",
    "longitudinal X [m]": "X longitudinal [m]",
    "radius [m]": "radio [m]",
    "span Y [m]": "envergadura Y [m]",
    "span [m]": "envergadura [m]",
    "x (longitudinal) [m]": "x (longitudinal) [m]",
    "x — longitudinal position [m]": "x — posición longitudinal [m]",
    "x-station [m]": "estación x [m]",
    "y (lateral) [m]": "y (lateral) [m]",
    "y/c  (sections offset vertically)": "y/c  (perfiles desplazados verticalmente)",
    "z — vertical [m]": "z — vertical [m]",
    "Specific thrust  SFn  [m/s]": "Empuje específico  SFn  [m/s]",
    "Specific thrust  SFn = F / mdot_total  [m/s]": "Empuje específico  SFn = F / mdot_total  [m/s]",
    "Overall (core) pressure ratio  OPR  [-]": "Relación de presiones global (núcleo)  OPR  [-]",
    "Mach number  $M_0$  [-]": "Número de Mach  $M_0$  [-]",
    "$\\alpha$ [deg]": "$\\alpha$ [grados]",
    # Titles
    "Aerodynamic & Propulsive Forces": "Fuerzas aerodinámicas y propulsivas",
    "Aerodynamic Coefficients": "Coeficientes aerodinámicos",
    "Airfoil Comparison: Original vs Optimized": "Comparación de perfiles: original frente a optimizado",
    "Airspeeds": "Velocidades",
    "Calculated Span Loading": "Distribución de sustentación calculada",
    "Control Surfaces & Tail Sizing": "Superficies de control y dimensionado de cola",
    "Cruise Design-Point Summary": "Resumen del punto de diseño en crucero",
    "Design evolution (planform)": "Evolución del diseño (planta)",
    "Dynamic modes — s-plane": "Modos dinámicos — plano s",
    "Empennage Wireframe (H-Stab & V-Stab)": "Estructura alámbrica del empenaje (estabilizadores horizontal y vertical)",
    "FEM vs Torenbeek wing mass": "Masa alar: FEM frente a Torenbeek",
    "Flight Path": "Trayectoria de vuelo",
    "Fuselage Wireframe": "Estructura alámbrica del fuselaje",
    "How the 2-D shortlist reshuffles in 3-D": "Cómo se reordena en 3-D la preselección 2-D",
    "MSES verification — real shock/viscous effects vs the VLM+Korn estimate": "Verificación con MSES — efectos reales de onda de choque y viscosos frente a la estimación VLM+Korn",
    "Mass Distribution — Plan View  (bubble area scales with mass)": "Distribución de masas — vista en planta  (el área de cada círculo escala con la masa)",
    "Matching Chart - Design Space": "Diagrama de adaptación - espacio de diseño",
    "Mission Profile": "Perfil de misión",
    "Model Comparison -- shared variables across AeroSandbox / SUAVE / MSES": "Comparación de modelos: variables comunes entre AeroSandbox, SUAVE y MSES",
    "Natural frequencies (bending)": "Frecuencias naturales (flexión)",
    "On-Design Cycle Station Temperatures": "Temperaturas por estación del ciclo de diseño",
    "Operational Limits": "Límites operativos",
    "Payload-Range Diagram": "Diagrama carga-alcance",
    "Pitching Moment vs Lift": "Momento de cabeceo frente a sustentación",
    "Random-vibration RMS check": "Comprobación RMS de vibración aleatoria",
    "Required vs Available": "Requerido frente a disponible",
    "Section shapes — top picks vs current": "Formas de perfil — mejores candidatos frente al actual",
    "Side profile (decks + payload + CG)": "Perfil lateral (cubiertas, carga de pago y CG)",
    "Sine-sweep frequency response (tip)": "Respuesta en frecuencia a barrido senoidal (punta)",
    "Span Loading (Lift Distribution) comparison": "Comparación de la distribución de sustentación en envergadura",
    "Spanwise deflection (analytical, Euler-Bernoulli)": "Flecha en envergadura (analítica, Euler-Bernoulli)",
    "Spar Cap Margin of Safety (analytical)": "Margen de seguridad del cordón del larguero (analítico)",
    "Stability Number Line (% MAC)": "Recta de estabilidad (% MAC)",
    "Stiffness & moment distribution": "Distribución de rigidez y momento",
    "Top airfoils for this design  (★ = real reference section)": "Mejores perfiles para este diseño  (★ = perfil de referencia real)",
    "Top view": "Vista en planta",
    "Trade map — L/D vs fuel capacity": "Mapa de compromiso — L/D frente a capacidad de combustible",
    "V-stab side view": "Vista lateral del estabilizador vertical",
    "Weight & Balance / CG Operational Envelope": "Pesos y centrado / envolvente operativa de CG",
    "Wing Fuel-Volume Check": "Comprobación del volumen de combustible del ala",
    "Wing Wireframe": "Estructura alámbrica del ala",
    "Wingbox planform": "Planta del cajón alar",
    "Nacelle profile silhouette\n(labels = editable (x-station, radius-fraction) points)": "Silueta del perfil de la góndola\n(las etiquetas son puntos editables (estación x, fracción de radio))",
    "2-D proxy L/D (isolated section)": "L/D del modelo 2-D (perfil aislado)",
    "3-D wing L/D (this design, cruise)": "L/D del ala 3-D (este diseño, en crucero)",
    # Legend entries
    "Available runway": "Pista disponible",
    "Bags/cargo ULD": "ULD de equipaje/carga",
    "CD compressibility": "CD de compresibilidad",
    "CD induced": "CD inducida",
    "CD miscellaneous": "CD varios",
    "CD parasite": "CD parásita",
    "CD total": "CD total",
    "CD wave": "CD de onda",
    "CD0 (parasite)": "CD0 (parásita)",
    "Caution (VC..VD)": "Precaución (VC..VD)",
    "Cm  (about aero CG)": "Cm  (respecto al CG aerodinámico)",
    "Cruise (T/W0 floor)": "Crucero (mínimo de T/W0)",
    "Cruise L/D": "L/D en crucero",
    "Destination": "Destino",
    "Fuel loading": "Carga de combustible",
    "Fuselage": "Fuselaje",
    "Galley": "Office",
    "H-Stab (root)": "Estab. horizontal (raíz)",
    "Ideal Elliptical Loading": "Distribución elíptica ideal",
    "Induced (CDi)": "Inducida (CDi)",
    "Lav": "Aseo",
    "Lower surface": "Intradós",
    "M = 1 (sonic)": "M = 1 (sónico)",
    "MSES (Stage 3, real shock/viscous)": "MSES (etapa 3, onda de choque y viscosidad reales)",
    "Miles equation": "Ecuación de Miles",
    "NASTRAN SOL 103 (nearest-frequency match)": "NASTRAN SOL 103 (frecuencia más próxima)",
    "NASTRAN random": "NASTRAN aleatorio",
    "Never exceed": "Nunca exceder",
    "Normal (<= VC)": "Normal (<= VC)",
    "Optimized": "Optimizado",
    "Origin": "Origen",
    "Parasite (CD0)": "Parásita (CD0)",
    "Payload loading": "Carga de pago",
    "Rayleigh (analytical)": "Rayleigh (analítico)",
    "Real reference section": "Perfil de referencia real",
    "Rudder": "Timón de dirección",
    "Structural margin (limit..ultimate)": "Margen estructural (límite..último)",
    "Trim  Cm = 0": "Equilibrado  Cm = 0",
    "Upper surface": "Extradós",
    "VLM + Korn (Stage 2)": "VLM + Korn (etapa 2)",
    "Wave (CDwave)": "Onda (CDwave)",
    "Wing (root)": "Ala (raíz)",
    "best so far": "mejor hasta ahora",
    "design CL": "CL de diseño",
    "evaluation": "evaluación",
    "induced (VLM)": "inducida (VLM)",
    "total (corrected)": "total (corregido)",
    "valid evaluation #": "evaluación válida n.º",
    "$\\eta_o = \\eta_{th}\\cdot\\eta_p$ (overall)": "$\\eta_o = \\eta_{th}\\cdot\\eta_p$ (global)",
    "$\\eta_p$ (propulsive)": "$\\eta_p$ (propulsivo)",
    "$\\eta_{th}$ (thermal)": "$\\eta_{th}$ (térmico)",
    "|H(f)| at tip": "|H(f)| en la punta",
}

CATALOG = {}
CATALOG.update(_LABELS)
CATALOG.update(_HELP)
CATALOG.update(_FIGURES)
