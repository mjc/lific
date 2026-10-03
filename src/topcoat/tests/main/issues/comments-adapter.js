const {product} = require('./harness');
function rootFixture(items=[]){
 const nodes=new Map(['[data-comment-thread]','[data-comment-count]','[data-comments-older]'].map(key=>[key,{innerHTML:'',textContent:'',hidden:false,disabled:false}]));
 const root={dataset:{issueId:'7',commentEnabled:'false'},ownerDocument:{activeElement:null},isConnected:true,_collabGeneration:1,_comments:items,_commentEdits:new Map(),
  querySelector(selector){if(nodes.has(selector))return nodes.get(selector);const id=selector.match(/^#comment-(\d+)$/)?.[1];return id&&root._comments.some(c=>c.id===Number(id))?{classList:{add(){}},scrollIntoView(){}}:null;},
  querySelectorAll(){return [];},contains(){return false;}};
 return root;
}
function collaboration(fetcher,location={hash:'',search:''}){
 return product('issue_detail/collaboration/assets/collaboration.js','LificTopcoatIssueCollaboration',source=>source.replace('mount,commentMarkup,','mergeComments,reconcileComments,readCommentWindow,fetchCommentPage,refreshComments,renderCommentList,mount,commentMarkup,'),{
  location,lificSession:{state:{user:{id:3},publicProject:null},request:fetcher},
 });
}
async function pageRequest(route, options){
 const response=await globalThis.fetch(`/api${route}`,options);let data;try{data=await response.json();}catch{data=null;}
 return response.ok?{ok:true,data,headers:response.headers}:{ok:false,error:data?.error||'offline',status:response.status};
}
async function listComments(id,before=null){
 const root=rootFixture();root.dataset.issueId=String(id);
 const api=collaboration(pageRequest);
 try{await api.refreshComments(root,1,before);return {ok:true,data:{items:root._comments,hasMore:root._commentsHasOlder,nextCursor:root._nextCommentCursor}};}catch(error){return {ok:false,error:error.message,status:null};}
}
async function prependOlderComments(existing,incoming){
 const root=rootFixture(existing);
 const api=collaboration(async()=>({ok:true,data:[...incoming].reverse()}));
 await api.refreshComments(root,1,{created_at:existing[0]?.created_at||'',id:existing[0]?.id||1});return root._comments;
}
async function upsertComment(existing,incoming){return collaboration().mergeComments(existing,[incoming]);}
async function removeComment(existing,id){const root=rootFixture();const api=collaboration(async()=>({ok:true,data:existing.filter(c=>c.id!==id).reverse()}));await api.refreshComments(root,1);return root._comments;}
async function compareComments(a,b){if(a.id===b.id&&a.created_at===b.created_at)return 0;const rows=await prependOlderComments([a,b],[]);return rows.findIndex(c=>c.id===a.id)-rows.findIndex(c=>c.id===b.id);}
async function olderCursor(existing){
 const root=rootFixture(),remaining=[...existing].reverse();
 const api=collaboration(async()=>{const data=remaining.splice(0,50);return {ok:true,data,headers:new Headers({'x-comment-has-more':String(remaining.length>0)})};});
 await api.refreshComments(root,1);
 while(remaining.length)await api.refreshComments(root,1,root._nextCommentCursor);
 return root._nextCommentCursor;
}
async function loadCommentWindow(fetchPage,minRows,pageSize=50,budget=10){
 const root=rootFixture();
 const api=collaboration(async route=>{const query=new URL(route,'http://localhost').searchParams;const before=query.has('before_id')?{id:Number(query.get('before_id')),created_at:query.get('before_created_at')}:null;
  const result=await fetchPage(before,Number(query.get('limit'))-1);if(!result.ok)return result;
  return {ok:true,data:[...result.data.items].reverse(),headers:new Headers({'x-comment-has-more':String(result.data.hasMore)})};});
 try{return {ok:true,data:await api.readCommentWindow(root,1,minRows)};}catch(error){return {ok:false,error:error.message,status:null};}
}
async function reconcileCommentWindow(onScreen,refreshed){
 // The refresh and local-fold boundaries intentionally have different duplicate
 // ownership: older pages keep newer on-screen rows; committed edits replace them.
 return collaboration().reconcileComments(onScreen,refreshed);
}
function canManageComment(comment,user,enabled){const api=product('issue_detail/collaboration/assets/collaboration.js','LificTopcoatIssueCollaboration',v=>v,{lificSession:{state:{user}}});return api.commentMarkup(comment,enabled).includes('data-comment-edit=');}
function commentWasEdited(comment){return collaboration(async()=>({ok:true,data:[]})).commentMarkup(comment,false).includes('edited');}
module.exports={rootFixture,collaboration,listComments,prependOlderComments,upsertComment,removeComment,compareComments,olderCursor,loadCommentWindow,reconcileCommentWindow,canManageComment,commentWasEdited,
 COMMENT_PAGE_SIZE:50,COMMENT_REFRESH_PAGE_BUDGET:10,COMMENT_REFRESH_TRANSFER_LIMIT:500,COMMENT_WINDOW_RETRY_LIMIT:3,ANCHOR_AUTO_PAGE_BUDGET:10,ANCHOR_AUTO_SEARCH_LIMIT:500};
