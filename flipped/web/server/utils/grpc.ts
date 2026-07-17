import { once } from "node:events";
import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import {
  parseCommandResponse,
  parseCreateResponse,
  parseSnapshotResponse,
} from "./protocol";
import type {
  ExaminerSnapshot,
  ParticipantRole,
  TestTakerSnapshot,
} from "#shared/session";

interface ExaminationClient extends grpc.Client {
  createSession(
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ): grpc.ClientWritableStream<Record<string, unknown>>;
  getTestTakerSessionSnapshot(
    request: object,
    metadata: grpc.Metadata,
    options: grpc.CallOptions,
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ): grpc.ClientUnaryCall;
  getExaminerSessionSnapshot(
    request: object,
    metadata: grpc.Metadata,
    options: grpc.CallOptions,
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ): grpc.ClientUnaryCall;
  startSession(
    request: object,
    metadata: grpc.Metadata,
    options: grpc.CallOptions,
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ): grpc.ClientUnaryCall;
  advanceSession(
    request: object,
    metadata: grpc.Metadata,
    options: grpc.CallOptions,
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ): grpc.ClientUnaryCall;
  endSession(
    request: object,
    metadata: grpc.Metadata,
    options: grpc.CallOptions,
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ): grpc.ClientUnaryCall;
  watchTestTakerSession(
    request: object,
    metadata: grpc.Metadata,
  ): grpc.ClientReadableStream<unknown>;
  watchExaminerSession(
    request: object,
    metadata: grpc.Metadata,
  ): grpc.ClientReadableStream<unknown>;
}
type ExaminationClientConstructor = new (
  address: string,
  credentials: grpc.ChannelCredentials,
  options?: object,
) => ExaminationClient;
let client: ExaminationClient | undefined;

export function examinationClient(): ExaminationClient {
  if (client) return client;
  const config = useRuntimeConfig();
  const definition = protoLoader.loadSync(String(config.grpcProtoPath), {
    keepCase: false,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true,
  });
  const root = grpc.loadPackageDefinition(definition) as unknown as {
    flipped: {
      examination: { v1: { ExaminationService: ExaminationClientConstructor } };
    };
  };
  client = new root.flipped.examination.v1.ExaminationService(
    String(config.grpcEndpoint),
    grpc.credentials.createInsecure(),
    {
      "grpc.max_send_message_length": Number(config.maxUploadBytes) + 65_536,
      "grpc.max_receive_message_length": 4 * 1024 * 1024,
    },
  );
  return client;
}

export function closeExaminationClient(): void {
  client?.close();
  client = undefined;
}

export function bearerMetadata(
  token: string,
  requestId?: string,
): grpc.Metadata {
  const metadata = new grpc.Metadata();
  metadata.set("authorization", `Bearer ${token}`);
  if (requestId) metadata.set("x-request-id", requestId);
  return metadata;
}

export async function createSession(
  chunks: AsyncIterable<Buffer>,
  extension: string,
  declaredSize: number,
  signal: AbortSignal,
) {
  const grpcClient = examinationClient();
  let call: grpc.ClientWritableStream<Record<string, unknown>>;
  const response = new Promise<unknown>((resolve, reject) => {
    call = grpcClient.createSession((error, value) =>
      error ? reject(error) : resolve(value),
    );
  });
  const activeCall = call!;
  const abort = () => activeCall.cancel();
  signal.addEventListener("abort", abort, { once: true });
  try {
    activeCall.write({
      metadata: {
        packageExtension: extension,
        declaredSizeBytes: String(declaredSize),
      },
      chunk: "metadata",
    });
    for await (const chunk of chunks) {
      if (signal.aborted) throw new Error("upload_cancelled");
      if (!activeCall.write({ data: chunk, chunk: "data" }))
        await once(activeCall, "drain");
    }
    activeCall.end();
    return parseCreateResponse(await response);
  } finally {
    signal.removeEventListener("abort", abort);
  }
}

function unary(
  method: (
    request: object,
    metadata: grpc.Metadata,
    options: grpc.CallOptions,
    callback: (error: grpc.ServiceError | null, response: unknown) => void,
  ) => grpc.ClientUnaryCall,
  request: object,
  metadata: grpc.Metadata,
): Promise<unknown> {
  return new Promise((resolve, reject) =>
    method(
      request,
      metadata,
      { deadline: Date.now() + 10_000 },
      (error, value) => (error ? reject(error) : resolve(value)),
    ),
  );
}

export async function getSnapshot(
  sessionId: string,
  role: "test_taker",
  token: string,
): Promise<TestTakerSnapshot>;
export async function getSnapshot(
  sessionId: string,
  role: "examiner",
  token: string,
): Promise<ExaminerSnapshot>;
export async function getSnapshot(
  sessionId: string,
  role: ParticipantRole,
  token: string,
): Promise<TestTakerSnapshot | ExaminerSnapshot> {
  const grpcClient = examinationClient();
  const metadata = bearerMetadata(token);
  const method =
    role === "test_taker"
      ? grpcClient.getTestTakerSessionSnapshot.bind(grpcClient)
      : grpcClient.getExaminerSessionSnapshot.bind(grpcClient);
  const raw = await unary(method, { sessionId }, metadata);
  return parseSnapshotResponse(raw, role as "test_taker") as
    TestTakerSnapshot | ExaminerSnapshot;
}

export async function executeCommand(
  name: "start" | "advance" | "end",
  sessionId: string,
  commandId: string,
  token: string,
): Promise<ExaminerSnapshot> {
  const grpcClient = examinationClient();
  const method =
    name === "start"
      ? grpcClient.startSession.bind(grpcClient)
      : name === "advance"
        ? grpcClient.advanceSession.bind(grpcClient)
        : grpcClient.endSession.bind(grpcClient);
  return parseCommandResponse(
    await unary(method, { sessionId, commandId }, bearerMetadata(token)),
  );
}

export function watchSession(
  sessionId: string,
  role: ParticipantRole,
  afterRevision: number,
  token: string,
): grpc.ClientReadableStream<unknown> {
  const grpcClient = examinationClient();
  const metadata = bearerMetadata(token);
  const request = { sessionId, afterRevision: String(afterRevision) };
  return role === "test_taker"
    ? grpcClient.watchTestTakerSession(request, metadata)
    : grpcClient.watchExaminerSession(request, metadata);
}
