export namespace config {
	
	export class TabState {
	    filePath: string;
	    profileId: string;
	    autoScroll: boolean;
	
	    static createFrom(source: any = {}) {
	        return new TabState(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.filePath = source["filePath"];
	        this.profileId = source["profileId"];
	        this.autoScroll = source["autoScroll"];
	    }
	}
	export class AppSettings {
	    pollIntervalMs: number;
	    bufferSize: number;
	    scrollBuffer: number;
	    theme: string;
	    fontSize: number;
	    showLineNumbers: boolean;
	    wordWrap: boolean;
	    restoreTabs: boolean;
	    activeProfile: string;
	    tabs: TabState[];
	    recentFiles: string[];
	
	    static createFrom(source: any = {}) {
	        return new AppSettings(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.pollIntervalMs = source["pollIntervalMs"];
	        this.bufferSize = source["bufferSize"];
	        this.scrollBuffer = source["scrollBuffer"];
	        this.theme = source["theme"];
	        this.fontSize = source["fontSize"];
	        this.showLineNumbers = source["showLineNumbers"];
	        this.wordWrap = source["wordWrap"];
	        this.restoreTabs = source["restoreTabs"];
	        this.activeProfile = source["activeProfile"];
	        this.tabs = this.convertValues(source["tabs"], TabState);
	        this.recentFiles = source["recentFiles"];
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
	

}

export namespace main {
	
	export class TabInfo {
	    id: string;
	    filePath: string;
	    fileName: string;
	    profile: string;
	
	    static createFrom(source: any = {}) {
	        return new TabInfo(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.id = source["id"];
	        this.filePath = source["filePath"];
	        this.fileName = source["fileName"];
	        this.profile = source["profile"];
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

}

