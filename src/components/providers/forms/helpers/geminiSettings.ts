interface BuildGeminiSettingsConfigArgs {
  env: Record<string, string>;
  config: Record<string, unknown>;
  manageAuthFiles: boolean;
  googleAccounts?: Record<string, unknown>;
  oauthCreds?: Record<string, unknown>;
}

export function buildGeminiSettingsConfig({
  env,
  config,
  manageAuthFiles,
  googleAccounts,
  oauthCreds,
}: BuildGeminiSettingsConfigArgs): Record<string, unknown> {
  const result: Record<string, unknown> = {
    env,
    config,
  };

  if (!manageAuthFiles) {
    return result;
  }

  const authFiles: Record<string, unknown> = {
    enabled: true,
  };

  if (googleAccounts) {
    authFiles.googleAccounts = googleAccounts;
  }
  if (oauthCreds) {
    authFiles.oauthCreds = oauthCreds;
  }

  result.authFiles = authFiles;
  return result;
}
