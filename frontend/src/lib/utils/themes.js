import { ListThemes, GetTheme } from '../../../wailsjs/go/main/App.js';

/**
 * Apply a theme's colors to the document root as CSS custom properties.
 * @param {object} theme - Theme object from Go backend
 * @param {string} mode - 'dark' or 'light'
 */
export function applyTheme(theme, mode) {
  if (!theme) return;

  const colors = mode === 'light' ? theme.light : theme.dark;
  if (!colors) return;

  const root = document.documentElement;

  // Set all CSS custom properties from theme colors
  for (const [key, value] of Object.entries(colors)) {
    if (value) {
      root.style.setProperty(`--${key}`, value);
    }
  }

  // Set color-scheme for native elements (scrollbars, form controls)
  root.style.setProperty('color-scheme', mode);

  // Set data-theme for any CSS selectors that depend on it
  root.setAttribute('data-theme', mode);
}

/**
 * Load and apply a theme by name and mode.
 * @param {string} themeName - Theme name, e.g. 'catppuccin'
 * @param {string} mode - 'dark' or 'light'
 */
export async function loadAndApplyTheme(themeName, mode) {
  try {
    const theme = await GetTheme(themeName);
    applyTheme(theme, mode);
  } catch (e) {
    console.error(`Failed to load theme "${themeName}":`, e);
    // Fall back to default CSS
    document.documentElement.setAttribute('data-theme', mode);
  }
}

/**
 * Get all available themes.
 * @returns {Promise<Array>} Array of theme objects
 */
export async function getAvailableThemes() {
  try {
    return await ListThemes();
  } catch (e) {
    console.error('Failed to list themes:', e);
    return [];
  }
}
