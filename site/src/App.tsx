import Masthead from './components/Masthead'
import Hero from './components/Hero'
import Overview from './components/Overview'
import Documentation from './components/Documentation'
import Downloads from './components/Downloads'
import ReleaseNotes from './components/ReleaseNotes'
import Colophon from './components/Colophon'

function App() {
  return (
    <div className="min-h-screen bg-base">
      <Masthead />
      <main>
        <Hero />
        <Overview />
        <Downloads />
        <Documentation />
        <ReleaseNotes />
      </main>
      <Colophon />
    </div>
  )
}

export default App
