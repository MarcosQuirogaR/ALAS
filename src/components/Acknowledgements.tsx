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

export default function Acknowledgements() {
  return (
    <section id="acknowledgements" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Acknowledgements</p>

        <div className="mt-6 max-w-[54ch]">
          <h2 className="font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
            People who made this
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
                  {c.names.join(' · ')}
                </p>
                <p className="mt-2 max-w-[62ch] text-[0.92rem] leading-[1.62] text-fg-dim">
                  {c.body}
                </p>
              </dd>
            </div>
          ))}
        </dl>
      </div>
    </section>
  )
}
