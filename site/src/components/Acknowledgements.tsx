import { withBase } from '../lib/base'

type Credit = { role: string; names: string[]; body: string }

const CREDITS: Credit[] = [
  {
    role: 'Principal collaborator',
    names: ['Javier García Rey'],
    body: 'Co-authored the original aircraft sizing program that ALAS grew out of, and contributed to the rest of the tooling alongside it. The project would not be what it is without his work on it.',
  },
  {
    role: 'Visual identity',
    names: ['Antón Ochoa Castro', 'Aarón Pérez Pardiñas'],
    body: 'Brought the aircraft to life in Blender. The renders they produced (including the one behind the home page) gave the project an essential part of its visual identity, and working with them was a genuine pleasure.',
  },
  {
    role: 'Structural analysis',
    names: [
      'Javier Garaizabal de la Montaña',
      'Breixo Salgado Fernández',
      'David Rodríguez El Bahri',
    ],
    body: 'Their contribution made it possible to determine the number of ribs an aircraft needs automatically, and with considerably better accuracy than before.',
  },
  {
    role: 'Testing and feedback',
    names: ['Zara Movilla Sesma', 'Pablo Magariños Docampo'],
    body: 'Put version 0.1.0 through real use and reported back on it: the kind of feedback that only comes from someone actually trying to get work done with the thing.',
  },
  {
    role: 'Academic supervision',
    names: [
      'Guillermo Rey González',
      'Uxía García Luis',
      'Carlos Ulloa Sande',
      'Pedro Orgeira Crespo',
    ],
    body: 'Supervised the project in its early stages and taught the courses it grew out of.',
  },
]

type Notice = { name: string; licence?: string; use: string }
type NoticeGroup = { title: string; intro?: string; items: Notice[] }

const REPO = 'https://github.com/MarcosQuirogaR/ALAS'

const NOTICES: NoticeGroup[] = [
  {
    title: 'Translated code',
    intro:
      'Derivative works: the upstream licence applies to the translated modules, and each file names its origin in a provenance header.',
    items: [
      { name: 'SUAVE 2.5.2', licence: 'LGPL-2.1, used under GPL-2.0-or-later', use: 'Mission analysis, weights, stability, turbofan network, vortex-lattice method, Chebyshev segment solver.' },
      { name: 'AeroSandbox 4.2.8', licence: 'MIT', use: 'Parametric geometry, vortex-lattice method, Torenbeek weight correlations, flight-dynamics modes.' },
      { name: 'NeuralFoil', licence: 'MIT', use: 'Airfoil polar surrogate and its trained weights.' },
      { name: 'MINPACK hybrd', licence: 'Public domain (Argonne National Laboratory)', use: 'Nonlinear solver behind the mission segment solve.' },
    ],
  },
  {
    title: 'Bundled data and fonts',
    items: [
      { name: 'UIUC Airfoil Coordinates Database', licence: 'Individual files, repacked unchanged', use: 'Airfoil coordinate library.' },
      { name: 'Noto Sans, Noto Sans Mono, Noto Sans Math', licence: 'SIL OFL 1.1', use: 'Interface text and figure rendering.' },
      { name: 'egui default fonts: Hack, Ubuntu-Light, Noto Emoji, emoji-icon-font', licence: 'MIT, Ubuntu Font Licence 1.0, SIL OFL 1.1, MIT', use: 'Embedded by the egui dependency.' },
      { name: 'NASA Blue Marble', licence: 'Public domain', use: 'Globe texture for route figures.' },
      { name: 'XFOIL 6.99 Orr-Sommerfeld map', licence: 'GPL-2.0-or-later', use: 'Transition data for an installed MSES; does not include MSES.' },
      { name: 'UAV component catalogue', licence: 'Manufacturer-published facts, sources cited per record', use: 'Electric propulsion sizing. Product names belong to their owners.' },
      { name: 'CADO airplane database v1.3 (ENAC)', licence: 'ODbL-1.0', use: 'Repository documentation only; not part of the program.' },
    ],
  },
  {
    title: 'Rust dependencies',
    intro:
      'Over 400 crates are linked into the executable, mostly under MIT or Apache-2.0, with Unicode-3.0, BSD, ISC, Zlib, BSL-1.0, MPL-2.0 and CC0-1.0 entries. The complete generated list and licence texts ship with every release and in Help > About ALAS.',
    items: [],
  },
  {
    title: 'Programs bundled in the Windows package',
    items: [
      { name: 'AVL 3.52', licence: 'GPL-2.0', use: 'Unchanged executable run as a separate process, shipped with its source and licence text.' },
      { name: 'NASTRAN-95', licence: 'NASA Open Source Agreement 1.3', use: 'Only when the complete reviewed source and notice set is staged; run as a separate process.' },
    ],
  },
  {
    title: 'Programs you supply',
    intro:
      'ALAS does not include or redistribute these. Each runs as a separate process, and you install it yourself.',
    items: [
      { name: 'MSES', licence: 'Per-seat licence from MIT', use: 'Viscous airfoil analysis.' },
      { name: 'MSC Nastran and MSC Patran', licence: 'Your own licence', use: 'Structural analysis and post-processing.' },
      { name: 'OpenVSP and VSPAERO', use: 'Geometry export and independent aerodynamic checks.' },
      { name: 'OpenFOAM, Gmsh and ParaView', use: 'Optional airfoil CFD.' },
      { name: 'FLOWUnsteady and Julia', use: 'Optional unsteady analysis.' },
    ],
  },
  {
    title: 'Downloaded at your request',
    intro: 'Each download asks for your consent first. ALAS does not redistribute any of it.',
    items: [
      { name: 'X-Plane format navigation data', use: 'Airway routing, from the third-party GitHub mirror mcantsin/x-plane-navdata. Never shipped in a release.' },
      { name: 'APC propeller performance data', use: 'UAV propeller model, from APC Propellers (apcprop.com performance-data page). Never shipped in a release.' },
      { name: 'OpenVSP preview runtime: CPython 3.13.7, OpenVSP Python bindings, NumPy 2.3.3', use: 'Native OpenVSP screenshots, from their official distribution points. Never shipped in a release.' },
    ],
  },
  {
    title: 'This website',
    items: [
      { name: 'React and React DOM', licence: 'MIT', use: 'Landing page.' },
      { name: 'Tailwind CSS', licence: 'MIT', use: 'Styling.' },
      { name: 'IBM Plex Sans and Mono, Space Grotesk', licence: 'SIL OFL 1.1', use: 'Typefaces, self-hosted via Fontsource (MIT packaging).' },
      { name: 'MkDocs', licence: 'BSD-2-Clause', use: 'Documentation generator.' },
      { name: 'Material for MkDocs, PyMdown Extensions', licence: 'MIT', use: 'Documentation theme and extensions.' },
      { name: 'Python-Markdown', licence: 'BSD-3-Clause', use: 'Markdown processing.' },
      { name: 'MathJax 3', licence: 'Apache-2.0', use: 'Equations, version 3.2.2, bundled with the docs (no external server).' },
    ],
  },
]

export default function Acknowledgements() {
  return (
    <section id="acknowledgements" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <div className="flex items-center justify-between">
          <p className="section-mark">Acknowledgements</p>
          <a href={withBase('')} className="font-mono text-[0.75rem] text-accent hover:underline">
            ← Back to Overview
          </a>
        </div>

        <div className="mt-6 max-w-[54ch]">
          <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            Contributors &amp; acknowledgements
          </h2>
          <p className="mt-5 text-[1rem] leading-[1.65] text-fg">
            ALAS carries one name on the design, but a good deal of it
            exists because other people gave it their time and their expertise.
          </p>
        </div>

        <dl className="mt-12 border-t border-rule">
          {CREDITS.map((c) => (
            <div
              key={c.role}
              className="grid gap-x-10 gap-y-3 border-b border-rule py-7 lg:grid-cols-[13rem_1fr]"
            >
              <dt className="font-mono text-[0.68rem] uppercase tracking-[0.13em] text-fg-dim lg:pt-1">
                {c.role}
              </dt>
              <dd>
                <p className="text-[1rem] font-semibold text-fg-strong">
                  {c.names.join(', ')}
                </p>
                <p className="mt-2 max-w-[62ch] text-[0.92rem] leading-[1.62] text-fg-dim">
                  {c.body}
                </p>
              </dd>
            </div>
          ))}
        </dl>

        <div className="mt-16 max-w-[54ch]">
          <p className="section-mark">Licence and third-party notices</p>
          <h2 className="mt-6 text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            Licence
          </h2>
          <p className="mt-5 text-[1rem] leading-[1.65] text-fg">
            ALAS is free software under the GNU Affero General Public License,
            version 3 or any later version (AGPL-3.0-or-later), copyright 2026
            Marcos Quiroga Rodr&iacute;guez. It comes with no warranty. The complete
            source code of every release is at{' '}
            <a href={REPO} className="text-accent hover:underline">
              github.com/MarcosQuirogaR/ALAS
            </a>
            ; the licence text is in the LICENSE file there and in every release
            package. The name and logo ALAS are not licensed by the AGPL.
          </p>
          <p className="mt-4 text-[1rem] leading-[1.65] text-fg">
            Aircraft, engine and manufacturer names (Airbus, Boeing, McDonnell
            Douglas, Embraer, COMAC, ATR and others) belong to their owners and
            are used only to identify the modelled type. Preset data come from
            public documents. ALAS is not affiliated with or endorsed by any
            of them, and its results are preliminary estimates, not
            manufacturer data.
          </p>
          <p className="mt-4 text-[1rem] leading-[1.65] text-fg">
            Full notices:{' '}
            <a href={REPO + '/blob/main/THIRD-PARTY-NOTICES.md'} className="text-accent hover:underline">
              THIRD-PARTY-NOTICES.md
            </a>
            ,{' '}
            <a href={REPO + '/blob/main/THIRD-PARTY-CRATES.md'} className="text-accent hover:underline">
              THIRD-PARTY-CRATES.md
            </a>{' '}
            and the{' '}
            <a href={withBase('docs/licensing/')} className="text-accent hover:underline">
              licensing page
            </a>{' '}
            of the documentation.
          </p>
        </div>

        <div className="mt-14 border-t border-rule">
          {NOTICES.map((g) => (
            <div key={g.title} className="grid gap-x-10 gap-y-3 border-b border-rule py-7 lg:grid-cols-[13rem_1fr]">
              <h3 className="font-mono text-[0.68rem] uppercase tracking-[0.13em] text-fg-dim lg:pt-1">
                {g.title}
              </h3>
              <div>
                {g.intro && (
                  <p className="max-w-[62ch] text-[0.92rem] leading-[1.62] text-fg-dim">{g.intro}</p>
                )}
                <ul className="mt-2 space-y-3">
                  {g.items.map((n) => (
                    <li key={n.name}>
                      <p className="text-[0.95rem] font-semibold text-fg-strong">
                        {n.name}{' '}
                        {n.licence && (
                          <span className="font-mono text-[0.78rem] font-normal text-accent">{n.licence}</span>
                        )}
                      </p>
                      <p className="max-w-[62ch] text-[0.88rem] leading-[1.6] text-fg-dim">{n.use}</p>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
