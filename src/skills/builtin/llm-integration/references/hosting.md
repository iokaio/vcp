# Cloud-hosted model endpoints

VCP-authored pointer notes, checked 2026-09-29 against Amazon Bedrock, Anthropic platform, Google Cloud and Microsoft Learn documentation. Load when the application reaches a model through a cloud platform rather than the model vendor's direct API. Availability changes often; everything below is a list of what to check, not a compatibility guarantee.

## What differs from a direct API

- **Authentication:** cloud identity (AWS credential chain and IAM/SigV4, Google Application Default Credentials, Microsoft Entra ID) or a platform-issued key, not the vendor's API key. Use the application's existing credential mechanism; do not add long-lived keys.
- **Model identity:** platform identifiers differ from vendor identifiers (provider prefixes, version suffixes, inference-profile IDs, or an Azure deployment name passed as `model`). Keep the configured value; never derive it from an example.
- **Region and access:** models are enabled per account/project and vary by region and endpoint type (global, multi-region, regional). Data-residency needs can require a regional endpoint.
- **Feature subset:** vendor features can be missing or late on a platform. Anthropic's platform pages, for example, list endpoints such as Message Batches as not available on Bedrock or Google Cloud. Check the platform's feature list for each feature the code uses.
- **Quotas, logging and retention** are governed by the platform account.

## Amazon Bedrock

- **Converse API** (model-agnostic): tools in `toolConfig.tools[].toolSpec` with `name`, `description` and `inputSchema.json`; the model replies with `toolUse` blocks carrying `toolUseId` and `stopReason: "tool_use"`; return `toolResult` blocks with the matching `toolUseId` (and `status` for errors) in a user message.
- **InvokeModel** takes the model vendor's native body; check that vendor's Bedrock page for required fields.
- Anthropic also documents a Messages-shaped Bedrock endpoint for newer Claude models with standard SSE streaming; the older InvokeModel/Converse path remains for earlier models. Check which one the application targets.
- Cross-region inference profiles route within a geography; check model access and region tables before changing region.

## Google Cloud (Vertex AI / Agent Platform)

- Authenticate with Google Cloud credentials (for local development, Application Default Credentials).
- Gemini: the `google-genai` SDK targets Google Cloud through its Vertex/Enterprise option with project and location instead of an API key.
- Claude: the request is Messages-shaped, but `model` is in the endpoint URL rather than the body, and `anthropic_version` is sent in the body with the documented Vertex value. Anthropic's SDKs provide Vertex clients that handle this.
- Endpoint type (global, multi-region, regional) affects availability and data residency.

## Microsoft Azure (Foundry / Azure OpenAI)

- OpenAI SDKs can target `https://<resource>.openai.azure.com/openai/v1/` with an API key or an Entra ID token provider.
- `model` is the **deployment name** chosen in the resource, not a vendor model identifier.
- Support for tools, parallel tool calls and Responses vs Chat Completions varies by model, API, deployment type and version; Microsoft also documents a tool-description length limit. Check the model catalog entry.

## Official starting points

- Bedrock tool use: <https://docs.aws.amazon.com/bedrock/latest/userguide/tool-use-inference-call.html>
- Claude on Bedrock / Google Cloud: <https://platform.claude.com/docs/en/build-with-claude/claude-in-amazon-bedrock>, <https://platform.claude.com/docs/en/build-with-claude/claude-on-vertex-ai>
- Google Cloud generative AI: <https://cloud.google.com/vertex-ai/docs>
- Azure function calling: <https://learn.microsoft.com/azure/foundry/openai/how-to/function-calling>

When no authorized cloud profile is available, verify request construction with fixtures and report the live platform check as not run.
