export namespace ai {
	
	export class DeviceCodeResponse {
	    device_code: string;
	    user_code: string;
	    verification_uri: string;
	    expires_in: number;
	    interval: number;
	
	    static createFrom(source: any = {}) {
	        return new DeviceCodeResponse(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.device_code = source["device_code"];
	        this.user_code = source["user_code"];
	        this.verification_uri = source["verification_uri"];
	        this.expires_in = source["expires_in"];
	        this.interval = source["interval"];
	    }
	}

}

export namespace config {
	
	export class WindowState {
	    x: number;
	    y: number;
	    width: number;
	    height: number;
	    maximised: boolean;
	
	    static createFrom(source: any = {}) {
	        return new WindowState(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.x = source["x"];
	        this.y = source["y"];
	        this.width = source["width"];
	        this.height = source["height"];
	        this.maximised = source["maximised"];
	    }
	}
	export class TabState {
	    filePath: string;
	    profileId: string;
	    autoScroll: boolean;
	    label?: string;
	    color?: string;
	    position: number;
	
	    static createFrom(source: any = {}) {
	        return new TabState(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.filePath = source["filePath"];
	        this.profileId = source["profileId"];
	        this.autoScroll = source["autoScroll"];
	        this.label = source["label"];
	        this.color = source["color"];
	        this.position = source["position"];
	    }
	}
	export class AppSettings {
	    pollIntervalMs: number;
	    bufferSize: number;
	    scrollBuffer: number;
	    scrollSpeed: number;
	    smoothScroll: boolean;
	    theme: string;
	    themeMode: string;
	    fontSize: number;
	    showLineNumbers: boolean;
	    wordWrap: boolean;
	    restoreTabs: boolean;
	    activeProfile: string;
	    tabs: TabState[];
	    recentFiles: string[];
	    window: WindowState;
	    displayBackend: string;
	    disableDmabuf: boolean;
	    gpuPolicy: string;
	    disableUpdateCheck: boolean;
	    updateCheckIntervalHours: number;
	    aiProvider: string;
	    aiEndpoint: string;
	    aiKey: string;
	    aiModel: string;
	
	    static createFrom(source: any = {}) {
	        return new AppSettings(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.pollIntervalMs = source["pollIntervalMs"];
	        this.bufferSize = source["bufferSize"];
	        this.scrollBuffer = source["scrollBuffer"];
	        this.scrollSpeed = source["scrollSpeed"];
	        this.smoothScroll = source["smoothScroll"];
	        this.theme = source["theme"];
	        this.themeMode = source["themeMode"];
	        this.fontSize = source["fontSize"];
	        this.showLineNumbers = source["showLineNumbers"];
	        this.wordWrap = source["wordWrap"];
	        this.restoreTabs = source["restoreTabs"];
	        this.activeProfile = source["activeProfile"];
	        this.tabs = this.convertValues(source["tabs"], TabState);
	        this.recentFiles = source["recentFiles"];
	        this.window = this.convertValues(source["window"], WindowState);
	        this.displayBackend = source["displayBackend"];
	        this.disableDmabuf = source["disableDmabuf"];
	        this.gpuPolicy = source["gpuPolicy"];
	        this.disableUpdateCheck = source["disableUpdateCheck"];
	        this.updateCheckIntervalHours = source["updateCheckIntervalHours"];
	        this.aiProvider = source["aiProvider"];
	        this.aiEndpoint = source["aiEndpoint"];
	        this.aiKey = source["aiKey"];
	        this.aiModel = source["aiModel"];
	    }
	
		convertValues(a: any, classs: any, asMap: boolean = false): any {
		    if (!a) {
		        return a;
		    }
		    if (a.slice && a.map) {
		        return (a as any[]).map(elem => this.convertValues(elem, classs));
		    } else if ("object" === typeof a) {
		        if (asMap) {
		            for (const key of Object.keys(a)) {
		                a[key] = new classs(a[key]);
		            }
		            return a;
		        }
		        return new classs(a);
		    }
		    return a;
		}
	}
	export class Rule {
	    id: string;
	    name: string;
	    pattern: string;
	    matchType: string;
	    foreground: string;
	    background: string;
	    bold: boolean;
	    italic: boolean;
	    enabled: boolean;
	    priority: number;
	
	    static createFrom(source: any = {}) {
	        return new Rule(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.id = source["id"];
	        this.name = source["name"];
	        this.pattern = source["pattern"];
	        this.matchType = source["matchType"];
	        this.foreground = source["foreground"];
	        this.background = source["background"];
	        this.bold = source["bold"];
	        this.italic = source["italic"];
	        this.enabled = source["enabled"];
	        this.priority = source["priority"];
	    }
	}
	export class Profile {
	    name: string;
	    rules: Rule[];
	
	    static createFrom(source: any = {}) {
	        return new Profile(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.name = source["name"];
	        this.rules = this.convertValues(source["rules"], Rule);
	    }
	
		convertValues(a: any, classs: any, asMap: boolean = false): any {
		    if (!a) {
		        return a;
		    }
		    if (a.slice && a.map) {
		        return (a as any[]).map(elem => this.convertValues(elem, classs));
		    } else if ("object" === typeof a) {
		        if (asMap) {
		            for (const key of Object.keys(a)) {
		                a[key] = new classs(a[key]);
		            }
		            return a;
		        }
		        return new classs(a);
		    }
		    return a;
		}
	}
	
	
	export class ThemeColors {
	    "bg-primary": string;
	    "bg-secondary": string;
	    "bg-surface": string;
	    "bg-hover": string;
	    "text-primary": string;
	    "text-secondary": string;
	    "text-muted": string;
	    accent: string;
	    "accent-hover": string;
	    border: string;
	    danger: string;
	    success: string;
	    warning: string;
	    "tab-active": string;
	    "tab-inactive": string;
	    "badge-color": string;
	    "scrollbar-track": string;
	    "scrollbar-thumb": string;
	
	    static createFrom(source: any = {}) {
	        return new ThemeColors(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this["bg-primary"] = source["bg-primary"];
	        this["bg-secondary"] = source["bg-secondary"];
	        this["bg-surface"] = source["bg-surface"];
	        this["bg-hover"] = source["bg-hover"];
	        this["text-primary"] = source["text-primary"];
	        this["text-secondary"] = source["text-secondary"];
	        this["text-muted"] = source["text-muted"];
	        this.accent = source["accent"];
	        this["accent-hover"] = source["accent-hover"];
	        this.border = source["border"];
	        this.danger = source["danger"];
	        this.success = source["success"];
	        this.warning = source["warning"];
	        this["tab-active"] = source["tab-active"];
	        this["tab-inactive"] = source["tab-inactive"];
	        this["badge-color"] = source["badge-color"];
	        this["scrollbar-track"] = source["scrollbar-track"];
	        this["scrollbar-thumb"] = source["scrollbar-thumb"];
	    }
	}
	export class Theme {
	    name: string;
	    displayName: string;
	    dark: ThemeColors;
	    light: ThemeColors;
	    builtIn: boolean;
	
	    static createFrom(source: any = {}) {
	        return new Theme(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.name = source["name"];
	        this.displayName = source["displayName"];
	        this.dark = this.convertValues(source["dark"], ThemeColors);
	        this.light = this.convertValues(source["light"], ThemeColors);
	        this.builtIn = source["builtIn"];
	    }
	
		convertValues(a: any, classs: any, asMap: boolean = false): any {
		    if (!a) {
		        return a;
		    }
		    if (a.slice && a.map) {
		        return (a as any[]).map(elem => this.convertValues(elem, classs));
		    } else if ("object" === typeof a) {
		        if (asMap) {
		            for (const key of Object.keys(a)) {
		                a[key] = new classs(a[key]);
		            }
		            return a;
		        }
		        return new classs(a);
		    }
		    return a;
		}
	}
	

}

export namespace main {
	
	export class MemoryStats {
	    alloc: number;
	    totalAlloc: number;
	    sys: number;
	    numGC: number;
	
	    static createFrom(source: any = {}) {
	        return new MemoryStats(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.alloc = source["alloc"];
	        this.totalAlloc = source["totalAlloc"];
	        this.sys = source["sys"];
	        this.numGC = source["numGC"];
	    }
	}
	export class TabInfo {
	    id: string;
	    filePath: string;
	    fileName: string;
	    profile: string;
	    label: string;
	    color: string;
	    position: number;
	
	    static createFrom(source: any = {}) {
	        return new TabInfo(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.id = source["id"];
	        this.filePath = source["filePath"];
	        this.fileName = source["fileName"];
	        this.profile = source["profile"];
	        this.label = source["label"];
	        this.color = source["color"];
	        this.position = source["position"];
	    }
	}
	export class UpdateCheckResult {
	    updateAvailable: boolean;
	    latestVersion: string;
	    currentVersion: string;
	    url: string;
	    error: string;
	
	    static createFrom(source: any = {}) {
	        return new UpdateCheckResult(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.updateAvailable = source["updateAvailable"];
	        this.latestVersion = source["latestVersion"];
	        this.currentVersion = source["currentVersion"];
	        this.url = source["url"];
	        this.error = source["error"];
	    }
	}

}

export namespace tailer {
	
	export class Line {
	    number: number;
	    text: string;
	
	    static createFrom(source: any = {}) {
	        return new Line(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.number = source["number"];
	        this.text = source["text"];
	    }
	}
	export class SearchResult {
	    matchLineNumbers: number[];
	    totalMatches: number;
	    totalLines: number;
	
	    static createFrom(source: any = {}) {
	        return new SearchResult(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.matchLineNumbers = source["matchLineNumbers"];
	        this.totalMatches = source["totalMatches"];
	        this.totalLines = source["totalLines"];
	    }
	}

}

