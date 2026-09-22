// Arithmatex in "generic" mode emits \(...\) and \[...\] inside
// <span class="arithmatex">, so MathJax has to be told both the delimiters
// and to leave the rest of the page alone.
window.MathJax = {
  tex: {
    inlineMath: [["\\(", "\\)"]],
    displayMath: [["\\[", "\\]"]],
    processEscapes: true,
    processEnvironments: true,
  },
  options: {
    ignoreHtmlClass: ".*|",
    processHtmlClass: "arithmatex",
  },
}

// With instant navigation the page never reloads, so MathJax has to be told
// to typeset again after each swap. Without this, formulas render on a cold
// load and then stop appearing as soon as you click through the nav.
document$.subscribe(() => {
  MathJax.startup.output.clearCache()
  MathJax.typesetClear()
  MathJax.texReset()
  MathJax.typesetPromise()
})
