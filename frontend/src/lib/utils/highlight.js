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
  const sortedRules = [...rules].filter(r => r.enabled).sort((a, b) => a.priority - b.priority);
  
  for (const rule of sortedRules) {
    if (rule.matchType === 'line') {
      try {
        const re = new RegExp(rule.pattern, 'i');
        if (re.test(text)) {
          lineStyle = {
            color: rule.foreground || undefined,
            backgroundColor: rule.background || undefined,
            fontWeight: rule.bold ? 'bold' : undefined,
            fontStyle: rule.italic ? 'italic' : undefined,
          };
        }
      } catch (e) {
        // skip invalid regex
      }
    }
  }

  // Collect match-level highlights
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
    try {
      const re = new RegExp(rule.pattern, 'gi');
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

  // Sort matches by start position, then by priority (higher priority last = wins)
  allMatches.sort((a, b) => a.start - b.start || a.priority - b.priority);

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
