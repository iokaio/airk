// SPDX-License-Identifier: Apache-2.0
/** Non-executing bounded TOML inventory; never infers installation ownership. */
export interface TomlStatement {text:string;start:number;end:number;comments:string[]}
export interface TomlDeclaration {serverKey:string;containerPath:string[];fields:Record<string,unknown>}
const MAX_DECLARATIONS=512;
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
function fail(code:string):never{throw new Error(code);}
/** Scan logical TOML statements before interpreting MCP declarations. In
 * particular, multiline instruction strings cannot manufacture table headers. */
export function scanTomlStatements(source:string):TomlStatement[]{
    const statements:TomlStatement[]=[];let statement='',quote='',triple=false,escaped=false,comment=false,depth=0,start=0,commentText='';let comments:string[]=[];
    const finish=(end:number)=>{if(statement.trim()||comments.length)statements.push({text:statement.trim(),start,end,comments});statement='';comments=[];start=end;};
    for(let i=0;i<source.length;i++){
        const c=source[i];
        if(comment){if(c==='\n'){comment=false;comments.push(commentText.trimEnd());commentText='';if(depth===0)finish(i+1);else statement+='\n';}else commentText+=c;continue;}
        if(quote){
            statement+=c;
            if(escaped){escaped=false;continue;}
            if(quote==='"'&&c==='\\'){escaped=true;continue;}
            if(c===quote){
                if(!triple)quote='';
                else if(source.slice(i,i+3)===quote.repeat(3)){
                    let end=i+3;while(source[end]===quote)end++;
                    if(end-i>5)fail('invalid_toml_string');
                    statement+=source.slice(i+1,end);i=end-1;quote='';triple=false;
                }
            }
            continue;
        }
        if(c==='#'){comment=true;commentText='#';continue;}
        if(c==='"'||c==="'"){quote=c;triple=source.slice(i,i+3)===c.repeat(3);statement+=triple?source.slice(i,i+3):c;if(triple)i+=2;continue;}
        if(c==='['||c==='{')depth++;
        if(c===']'||c==='}')if(--depth<0)fail('invalid_toml_structure');
        if(c==='\n'&&depth===0)finish(i+1);else statement+=c;
    }
    if(quote||depth!==0)fail('unterminated_toml_structure');if(comment)comments.push(commentText.trimEnd());finish(source.length);return statements;
}
export class TomlValue {
    offset=0;
    constructor(readonly text:string){}
    space(){while(/\s/.test(this.text[this.offset]??'')&&this.offset<this.text.length)this.offset++;}
    string():string{
        const quote=this.text[this.offset],triple=this.text.slice(this.offset,this.offset+3)===quote.repeat(3);this.offset+=triple?3:1;
        let value='';if(triple){if(this.text.slice(this.offset,this.offset+2)==='\r\n')this.offset+=2;else if(this.text[this.offset]==='\n')this.offset++;}
        while(this.offset<this.text.length){
            if(this.text.slice(this.offset,this.offset+(triple?3:1))===quote.repeat(triple?3:1)){
                if(triple){let end=this.offset+3;while(this.text[end]===quote)end++;if(end-this.offset>5)fail('invalid_toml_string');value+=quote.repeat(end-this.offset-3);this.offset=end;}
                else this.offset++;
                return value;
            }
            const c=this.text[this.offset++];
            if(c!=='\\'||quote==="'"){if(!triple&&(c==='\n'||c==='\r'))fail('invalid_toml_string');value+=c;continue;}
            const next=this.text[this.offset++];
            if(triple&&/\s/.test(next)){while(this.offset<this.text.length&&/\s/.test(this.text[this.offset]))this.offset++;continue;}
            const escapes:Record<string,string>={b:'\b',t:'\t',n:'\n',f:'\f',r:'\r','"':'"','\\':'\\'};
            if(next in escapes){value+=escapes[next];continue;}
            if(next==='u'||next==='U'){
                const length=next==='u'?4:8,hex=this.text.slice(this.offset,this.offset+length);if(!new RegExp('^[0-9a-fA-F]{'+length+'}$').test(hex))fail('invalid_toml_escape');
                const code=Number.parseInt(hex,16);if(code>0x10ffff||(code>=0xd800&&code<=0xdfff))fail('invalid_toml_escape');value+=String.fromCodePoint(code);this.offset+=length;continue;
            }
            fail('unsupported_toml_escape');
        }
        return fail('unterminated_toml_string');
    }
    keys():string[]{
        const keys:string[]=[];
        while(true){this.space();let key:string;
            if(this.text[this.offset]==='"'||this.text[this.offset]==="'")key=this.string();
            else{const match=this.text.slice(this.offset).match(/^[A-Za-z0-9_-]+/);if(!match)fail('invalid_toml_key');key=match[0];this.offset+=key.length;}
            keys.push(key);if(keys.length>32)fail('toml_key_depth_bound');this.space();if(this.text[this.offset]!=='.')return keys;this.offset++;
        }
    }
    value(depth=0):unknown{
        if(depth>32)fail('toml_value_depth_bound');this.space();const c=this.text[this.offset];
        if(c==='"'||c==="'")return this.string();
        if(c==='['){this.offset++;const result:unknown[]=[];this.space();while(this.text[this.offset]!==']'){
            result.push(this.value(depth+1));this.space();if(this.text[this.offset]===','){this.offset++;this.space();}else if(this.text[this.offset]!==']')fail('invalid_toml_array');
            if(result.length>65536)fail('toml_array_bound');
        }this.offset++;return result;}
        if(c==='{'){this.offset++;const result:Record<string,unknown>=Object.create(null);this.space();while(this.text[this.offset]!=='}'){
            const keys=this.keys();if(this.text[this.offset++]!=='=')fail('invalid_inline_toml');setToml(result,keys,this.value(depth+1));this.space();
            if(this.text[this.offset]===','){this.offset++;this.space();}else if(this.text[this.offset]!=='}')fail('invalid_inline_toml');
        }this.offset++;return result;}
        const match=this.text.slice(this.offset).match(/^[^\s,\]}]+/);if(!match)fail('invalid_toml_value');this.offset+=match[0].length;
        const raw=match[0],number=raw.replaceAll('_','');if(raw==='true'||raw==='false')return raw==='true';
        if(/^(?:[+-]?(?:0|[1-9](?:_?\d)*)(?:\.\d(?:_?\d)*)?(?:[eE][+-]?\d(?:_?\d)*)?|0x[0-9a-fA-F](?:_?[0-9a-fA-F])*|0o[0-7](?:_?[0-7])*|0b[01](?:_?[01])*)$/.test(raw)){
            const value=Number(number);return Number.isFinite(value)&&(!Number.isInteger(value)||Number.isSafeInteger(value))?value:{tomlScalar:raw};
        }
        // Date/time and non-finite TOML values are retained as opaque scalars.
        // They can never match a launcher path or an argument vector; a guarded
        // writer still operates on original bytes and may require manual review.
        if(/^(?:[+-]?(?:inf|nan)|\d{4}-\d{2}-\d{2}(?:[Tt].*)?|\d{2}:\d{2}:\d{2}.*)$/.test(raw))return {tomlScalar:raw};
        return fail('unsupported_mcp_toml_value');
    }
    complete(){this.space();if(this.offset!==this.text.length)fail('unsupported_mcp_toml_value');}
}
function setToml(target:Record<string,unknown>,keys:string[],value:unknown):void{
    for(const key of keys.slice(0,-1)){if(target[key]===undefined)target[key]=Object.create(null);if(!object(target[key]))fail('conflicting_mcp_toml_field');target=target[key] as Record<string,unknown>;}
    const key=keys.at(-1)!;if(Object.hasOwn(target,key))fail('duplicate_mcp_toml_field');target[key]=value;
}
/** Serialize only representable values after the caller's ownership/policy
 * checks. Unknown scalars must not be reinterpreted while rewriting a server. */
export function encodeTomlValue(value:unknown):string{
    if(typeof value==='string'||typeof value==='boolean')return JSON.stringify(value);
    if(typeof value==='number'&&Number.isFinite(value))return String(value);
    if(Array.isArray(value))return '['+value.map(encodeTomlValue).join(',')+']';
    if(object(value)&&!Object.hasOwn(value,'tomlScalar'))return '{ '+Object.entries(value).map(([key,item])=>JSON.stringify(key)+' = '+encodeTomlValue(item)).join(', ')+' }';
    return fail('unsupported_owned_toml_value');
}
export function parseTomlDeclarations(source:string):TomlDeclaration[]{
    const root:Record<string,unknown>=Object.create(null),seen=new Set<string>(),sealed:string[][]=[];let table:string[]=[];
    for(const {text:statement} of scanTomlStatements(source)){
        if(!statement)continue;
        if(statement.startsWith('[')){
            const array=statement.startsWith('[['),end=array?']]':']';if(!statement.endsWith(end))fail('invalid_toml_header');
            const parser=new TomlValue(statement.slice(array?2:1,-end.length));table=parser.keys();parser.complete();
            if(table.includes('mcp_servers')){if(array)fail('unsupported_mcp_array_table');const key=JSON.stringify(table);if(seen.has(key))fail('duplicate_mcp_toml_table');seen.add(key);}
            continue;
        }
        const parser=new TomlValue(statement),keys=parser.keys();parser.space();if(parser.text[parser.offset++]!=='=')fail('invalid_toml_assignment');
        const full=[...table,...keys];if(!full.includes('mcp_servers'))continue;
        if(sealed.some(keys=>keys.length<=full.length&&keys.every((key,index)=>full[index]===key)))fail('conflicting_inline_mcp_toml');
        parser.space();const inline=parser.text[parser.offset]==='{';const value=parser.value();parser.complete();setToml(root,full,value);if(inline)sealed.push(full);
    }
    const rows:TomlDeclaration[]=[];
    const visit=(value:Record<string,unknown>,prefix:string[])=>{for(const [key,item] of Object.entries(value)){
        if(key==='mcp_servers'){
            if(!object(item))fail('invalid_mcp_server_group');for(const [serverKey,fields] of Object.entries(item)){
                if(!object(fields))fail('invalid_mcp_server_declaration');rows.push({serverKey,containerPath:[...prefix,key],fields});if(rows.length>MAX_DECLARATIONS)fail('configuration_declaration_bound');
            }
        }else if(object(item))visit(item,[...prefix,key]);
    }};visit(root,[]);return rows;
}
