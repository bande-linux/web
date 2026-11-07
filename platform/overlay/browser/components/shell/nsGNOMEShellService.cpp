/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsGNOMEShellService.h"

#include "mozilla/dom/Promise.h"
#include "nsIINIParser.h"

nsresult nsGNOMEShellService::Init() { return NS_OK; }

NS_IMPL_ISUPPORTS(nsGNOMEShellService, nsIGNOMEShellService, nsIShellService,
                  nsIToolkitShellService)

NS_IMETHODIMP
nsGNOMEShellService::IsDefaultBrowser(bool, bool* aIsDefaultBrowser) {
  *aIsDefaultBrowser = true;
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::IsDefaultForScheme(const nsACString&,
                                        bool* aIsDefaultBrowser) {
  *aIsDefaultBrowser = true;
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::SetDefaultBrowser(bool) { return NS_OK; }

NS_IMETHODIMP
nsGNOMEShellService::GetCanSetDesktopBackground(bool* aResult) {
  *aResult = false;
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::SetDesktopBackground(mozilla::dom::Element*, int32_t,
                                          const nsACString&) {
  return NS_ERROR_NOT_AVAILABLE;
}

NS_IMETHODIMP
nsGNOMEShellService::GetDesktopBackgroundColor(uint32_t* aColor) {
  *aColor = 0;
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::SetDesktopBackgroundColor(uint32_t) {
  return NS_ERROR_NOT_AVAILABLE;
}

NS_IMETHODIMP
nsGNOMEShellService::GetGSettingsString(const nsACString&, const nsACString&,
                                        nsACString& aResult) {
  aResult.Truncate();
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::SetGSettingsString(const nsACString&, const nsACString&,
                                        const nsACString&) {
  return NS_ERROR_NOT_AVAILABLE;
}

NS_IMETHODIMP
nsGNOMEShellService::GetArgv0(nsACString& aOutput) {
  aOutput.Truncate();
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::GetGlibPrgname(nsACString& aOutput) {
  aOutput.Truncate();
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::GetDesktopEntryStatus(
    const nsACString&, nsIGNOMEShellService::DesktopEntryStatus* aStatus) {
  *aStatus = nsIGNOMEShellService::DESKTOP_ENTRY_ABSENT;
  return NS_OK;
}

NS_IMETHODIMP
nsGNOMEShellService::RequestInstallDynamicLauncher(const nsACString&,
                                                   nsIINIParserWriter*,
                                                   mozIDOMWindowProxy*, JSContext*,
                                                   mozilla::dom::Promise**) {
  return NS_ERROR_NOT_AVAILABLE;
}

NS_IMETHODIMP
nsGNOMEShellService::RequestUninstallDynamicLauncher(const nsACString&,
                                                     JSContext*,
                                                     mozilla::dom::Promise**) {
  return NS_ERROR_NOT_AVAILABLE;
}
