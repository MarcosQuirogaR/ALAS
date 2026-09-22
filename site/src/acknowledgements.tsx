import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import Masthead from './components/Masthead'
import Acknowledgements from './components/Acknowledgements'
import Colophon from './components/Colophon'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <div className="min-h-screen bg-base">
      <Masthead />
      <main>
        <Acknowledgements />
      </main>
      <Colophon />
    </div>
  </StrictMode>,
)
