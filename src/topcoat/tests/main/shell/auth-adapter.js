const {controllerFixture}=require('./session-harness.js');
const changePassword=async payload=>controllerFixture().app.changePassword(payload.current_password,payload.new_password);
const revokeAllSessions=async()=>controllerFixture().app.signOutAll();
const me=async()=>controllerFixture().session.request('/auth/me');
module.exports={changePassword,revokeAllSessions,me};
