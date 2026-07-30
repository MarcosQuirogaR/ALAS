// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import React from 'react'
import {createRoot} from 'react-dom/client'
import './style.css'
import App from './App'
import {LangProvider} from './lib/i18n'

const container = document.getElementById('root')

const root = createRoot(container!)

root.render(
    <React.StrictMode>
        {/* Outside <App/> so the language is set before anything renders or
            fetches -- sidecar requests carry ?lang= from the first call. */}
        <LangProvider>
            <App/>
        </LangProvider>
    </React.StrictMode>
)
