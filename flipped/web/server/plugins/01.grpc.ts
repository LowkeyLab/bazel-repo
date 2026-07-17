import { closeExaminationClient, examinationClient } from "../utils/grpc";

export default defineNitroPlugin((nitroApp) => {
  examinationClient();
  nitroApp.hooks.hookOnce("close", () => closeExaminationClient());
});
