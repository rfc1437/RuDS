(() => {
  const toggle = document.querySelector('[data-blog-search-toggle]');
  const panel = document.querySelector('[data-blog-search-panel]');
  const root = document.querySelector('[data-blog-search-root]');

  if (!toggle || !panel || !root) {
    return;
  }

  let initialized = false;

  function initSearch() {
    if (initialized || typeof PagefindUI === 'undefined') {
      return;
    }
    initialized = true;
    var placeholder = root.getAttribute('data-search-placeholder') || 'Search...';
    var zeroResults = root.getAttribute('data-search-no-results') || 'No results found';
    new PagefindUI({
      element: root,
      showSubResults: true,
      showImages: false,
      translations: { placeholder: placeholder, zero_results: zeroResults }
    });
    var input = root.querySelector('input');
    if (input) {
      input.focus();
    }
  }

  toggle.addEventListener('click', function() {
    var isHidden = panel.hasAttribute('hidden');
    if (isHidden) {
      panel.removeAttribute('hidden');
      initSearch();
    } else {
      panel.setAttribute('hidden', '');
    }
  });

  document.addEventListener('click', function(e) {
    if (!panel.hasAttribute('hidden') && !panel.contains(e.target) && !toggle.contains(e.target)) {
      panel.setAttribute('hidden', '');
    }
  });

  document.addEventListener('keydown', function(e) {
    if (e.key === 'Escape' && !panel.hasAttribute('hidden')) {
      panel.setAttribute('hidden', '');
      toggle.focus();
    }
  });
})();