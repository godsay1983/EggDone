const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
function functions(file, names) {
  const source = fs.readFileSync(path.join(__dirname, '../src/lib/components', file), 'utf8');
  const script = source.slice(source.indexOf('<script lang="ts">') + 18, source.indexOf('</script>'));
  const ast = ts.createSourceFile(file + '.ts', script, ts.ScriptTarget.Latest, true);
  const nodes = ast.statements.filter(n => ts.isFunctionDeclaration(n) && names.includes(n.name?.text));
  assert.equal(nodes.length, names.length);
  return nodes.map(n => n.getText(ast)).join('\n');
}
const compile = code => ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const viewer = functions('NoteEditor.svelte', ['openAttachment', 'closeViewer', 'handleViewerKeydown']);
function fixture() {
  const revoked = [], pending = [];
  const h = new Function('onOpenAttachment', 'URL', 'localizedErrorMessage', compile(`
    let viewerRequest=0,viewerAttachment=null,viewerUrl='',viewerLoading=false,viewerError='';
    let attachmentManagerOpen=true,addMenuOpen=true;
    ${viewer}
    return {openAttachment,closeViewer,handleViewerKeydown,state:()=>({viewerAttachment,viewerUrl,viewerLoading,viewerError,attachmentManagerOpen,addMenuOpen})};
  `))(() => new Promise((resolve,reject) => pending.push({resolve,reject})), {revokeObjectURL:url=>revoked.push(url)}, String);
  return {h,pending,revoked};
}
(async()=>{
  const {h,pending,revoked}=fixture();
  const a=h.openAttachment({uuid:'a'});
  const event=()=>({key:'Escape',prevented:false,stopped:false,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;}});
  const escape=event();h.handleViewerKeydown(escape);assert.ok(escape.prevented&&escape.stopped);
  pending[0].resolve('blob:a');await a;assert.equal(h.state().viewerAttachment,null);assert.deepEqual(revoked,['blob:a']);
  const b=h.openAttachment({uuid:'b'}),c=h.openAttachment({uuid:'c'});
  pending[2].resolve('blob:c');await c;pending[1].resolve('blob:b');await b;
  assert.equal(h.state().viewerUrl,'blob:c');assert.ok(revoked.includes('blob:b'));
  h.handleViewerKeydown(event());assert.ok(revoked.includes('blob:c'));
  h.handleViewerKeydown(event());assert.equal(h.state().attachmentManagerOpen,false);assert.equal(h.state().addMenuOpen,true);
  h.handleViewerKeydown(event());assert.equal(h.state().addMenuOpen,false);
  const noOverlay=event();h.handleViewerKeydown(noOverlay);assert.equal(noOverlay.prevented,false);
  const d=h.openAttachment({uuid:'d'});h.closeViewer();pending[3].reject(Error('late failure'));await d;assert.equal(h.state().viewerError,'');

  const deletion=functions('TodoPanel.svelte',['deleteNote','requestNoteDeletion']);let failure='save',removed=0;
  const remove=new Function('flushAllNoteChanges','notes','setTimeout',compile(`
    const NOTE_DRAFT_UUID='draft';let selectedNoteUuid='source',deletedNote=null,noteUndoTimer=null;
    ${deletion}
    return {deleteNote,requestNoteDeletion,state:()=>selectedNoteUuid};
  `))(async()=>{if(failure==='save')throw Error('save failed');},{remove:async()=>{if(failure==='delete')throw Error('delete failed');removed++;return {uuid:'source'};}},()=>1);
  await assert.rejects(remove.deleteNote({uuid:'source'}),/save failed/);assert.equal(removed,0);assert.equal(remove.state(),'source');
  await remove.requestNoteDeletion({uuid:'source'});assert.equal(removed,0);assert.equal(remove.state(),'source');
  failure='delete';await assert.rejects(remove.deleteNote({uuid:'source'}),/delete failed/);assert.equal(remove.state(),'source');
  await remove.requestNoteDeletion({uuid:'source'});assert.equal(removed,0);assert.equal(remove.state(),'source');
  failure='';await remove.requestNoteDeletion({uuid:'source'});assert.equal(removed,1);assert.equal(remove.state(),null);
  const panel=fs.readFileSync(path.join(__dirname,'../src/lib/components/TodoPanel.svelte'),'utf8');
  assert.equal((panel.match(/onDelete=\{requestNoteDeletion\}/g)||[]).length,2);
  assert.ok(!panel.includes('onDelete={deleteNote}'));
  console.log('Production viewer race/overlay priority and delete save/failure boundaries passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
