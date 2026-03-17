/**
 * Strip Go-style inline flags (e.g. (?i), (?s), (?m)) from a regex pattern.
 * JavaScript RegExp doesn't support inline flags — they're passed as the
 * second argument instead. The highlight functions already use 'i' and 'gi'.
 */
function stripInlineFlags(pattern) {
  return pattern.replace(/\(\?[ismUu]+\)/g, '');
}

// Cache compiled RegExp objects to avoid recompilation on every line.
// Key: "pattern|flags" → RegExp
const regexCache = new Map();
const REGEX_CACHE_MAX = 200;

function getCachedRegex(pattern, flags) {
  const key = pattern + '|' + flags;
  let re = regexCache.get(key);
  if (re) {
    re.lastIndex = 0;
    return re;
  }
  re = new RegExp(stripInlineFlags(pattern), flags);
  if (regexCache.size >= REGEX_CACHE_MAX) {
    // Evict oldest entry
    const first = regexCache.keys().next().value;
    regexCache.delete(first);
  }
  regexCache.set(key, re);
  return re;
}

/**
 * Apply highlighting rules to a text line.
 * Returns an array of segments: { text, style }
 */
export function highlightLine(text, rules) {
  if (!rules || rules.length === 0) {
    return [{ text, style: {} }];
  }

  // Check for line-level rules first (highest priority wins)
  let lineStyle = null;
  let linePriority = -1;
  const sortedRules = [...rules].filter(r => r.enabled).sort((a, b) => a.priority - b.priority);
  
  for (const rule of sortedRules) {
    if (rule.matchType === 'line') {
      try {
        const re = getCachedRegex(rule.pattern, 'i');
        if (re.test(text)) {
          lineStyle = {
            color: rule.foreground || undefined,
            backgroundColor: rule.background || undefined,
            fontWeight: rule.bold ? 'bold' : undefined,
            fontStyle: rule.italic ? 'italic' : undefined,
          };
          linePriority = rule.priority;
        }
      } catch (e) {
        // skip invalid regex
      }
    }
  }

  // Collect match-level highlights (only those with priority >= active line rule)
  const matchRules = sortedRules.filter(r => r.matchType === 'match');
  if (matchRules.length === 0) {
    if (lineStyle) {
      return [{ text, style: lineStyle }];
    }
    return [{ text, style: {} }];
  }

  // Find all match positions
  let allMatches = [];
  for (const rule of matchRules) {
    if (lineStyle && rule.priority < linePriority) continue;
    try {
      const re = getCachedRegex(rule.pattern, 'gi');
      re.lastIndex = 0;
      let m;
      while ((m = re.exec(text)) !== null) {
        if (m[0].length === 0) break; // prevent infinite loop on zero-width matches
        allMatches.push({
          start: m.index,
          end: m.index + m[0].length,
          priority: rule.priority,
          style: {
            color: rule.foreground || undefined,
            backgroundColor: rule.background || undefined,
            fontWeight: rule.bold ? 'bold' : undefined,
            fontStyle: rule.italic ? 'italic' : undefined,
          }
        });
      }
    } catch (e) {
      // skip invalid regex
    }
  }

  if (allMatches.length === 0) {
    if (lineStyle) {
      return [{ text, style: lineStyle }];
    }
    return [{ text, style: {} }];
  }

  // Sort matches by start position, then by priority descending (higher priority wins)
  allMatches.sort((a, b) => a.start - b.start || b.priority - a.priority);

  // Build segments
  const segments = [];
  let pos = 0;
  const baseStyle = lineStyle || {};

  for (const match of allMatches) {
    if (match.start < pos) continue; // skip overlapping

    if (match.start > pos) {
      segments.push({ text: text.slice(pos, match.start), style: baseStyle });
    }
    segments.push({
      text: text.slice(match.start, match.end),
      style: { ...baseStyle, ...match.style }
    });
    pos = match.end;
  }

  if (pos < text.length) {
    segments.push({ text: text.slice(pos), style: baseStyle });
  }

  return segments;
}
