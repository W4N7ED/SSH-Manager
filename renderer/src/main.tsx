import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

// Interface fonts, bundled locally: the renderer's CSP forbids remote font hosts.
// Latin subsets only — the interface never renders other scripts.
import '@fontsource/archivo/latin-400.css';
import '@fontsource/archivo/latin-500.css';
import '@fontsource/archivo/latin-600.css';
import '@fontsource/archivo/latin-700.css';
import '@fontsource/archivo/latin-800.css';
import '@fontsource/jetbrains-mono/latin-400.css';
import '@fontsource/jetbrains-mono/latin-500.css';
import '@fontsource/jetbrains-mono/latin-700.css';

import './index.css';

// Un glisser-déposer relâché sur la page fait naviguer Chromium vers le contenu
// déposé — tirer une sélection de texte suffisait à faire disparaître
// l'interface. Les zones de dépôt d'onglets traitent l'évènement en amont, sur
// leurs propres éléments : neutraliser le comportement par défaut ici ne les
// empêche pas de fonctionner.
window.addEventListener('dragover', event => event.preventDefault());
window.addEventListener('drop', event => event.preventDefault());

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
