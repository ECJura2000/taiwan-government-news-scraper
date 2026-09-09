import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");
function luminance(hex: string) {
  const rgb = hex.slice(1).match(/../g)!.map(v => parseInt(v,16)/255).map(v => v <= .04045 ? v/12.92 : ((v+.055)/1.055)**2.4);
  return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722;
}
function contrast(a: string,b: string) { const x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05); }
describe("深淺色對比驗證", () => {
  const sections=[css.match(/:root \{([\s\S]*?)\}/)![1],css.match(/:root\[data-theme="dark"\] \{([\s\S]*?)\}/)![1]];
  sections.forEach((section,index) => {
    const colors=Object.fromEntries([...section.matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})/g)].map(m => [m[1],m[2]]));
    it(`${index ? '深色' : '淺色'}：文字及按鈕所有狀態至少 4.5:1`, () => {
      const pairs=[['text','bg'],['text','surface'],['muted','surface'],['muted','surface-alt'],['muted','selected'],['on-primary','primary'],['on-primary','primary-hover'],['on-secondary','secondary'],['on-secondary','secondary-hover'],['on-danger','danger'],['on-danger','danger-hover'],['on-disabled','disabled'],['success','success-bg'],['warning','warning-bg'],['error','error-bg'],['link','surface']];
      for(const [foreground,background] of pairs) expect(contrast(colors[foreground],colors[background]),`${foreground}/${background}`).toBeGreaterThanOrEqual(4.5);
    });
    it(`${index ? '深色' : '淺色'}：邊界及焦點至少 3:1`, () => {
      for(const [a,b] of [['border','surface'],['border','bg'],['focus','surface'],['focus','bg']]) expect(contrast(colors[a],colors[b]),`${a}/${b}`).toBeGreaterThanOrEqual(3);
    });
  });
});
