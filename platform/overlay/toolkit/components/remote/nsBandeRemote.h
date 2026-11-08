/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef nsBandeRemote_h__
#define nsBandeRemote_h__

#include "nsRemoteClient.h"
#include "nsRemoteServer.h"
#include "nsString.h"
#include "nsUnixRemoteServer.h"

struct BandeRemoteAddress {
  nsCString mName;
  nsCString mPath;
  nsCString mInterface;
};

bool BandeRemoteAddressFor(const char* aProfile, BandeRemoteAddress& aAddress);

class nsBandeRemoteServer final : public nsRemoteServer,
                                  public nsUnixRemoteServer {
 public:
  nsBandeRemoteServer() = default;
  ~nsBandeRemoteServer() override;

  nsresult Startup(const char* aAppName, const char* aProfileName) override;
  void Shutdown() override;

 private:
  static void Handle(const uint8_t* aCommand, size_t aLength);
};

class nsBandeRemoteClient final : public nsRemoteClient {
 public:
  explicit nsBandeRemoteClient(nsACString& aStartupToken)
      : mStartupToken(aStartupToken) {}

  nsresult Init() override { return NS_OK; }
  nsresult SendCommandLine(const char* aProgram, const char* aProfile,
                           int32_t argc, const char** argv,
                           bool aRaise) override;

 private:
  nsCString mStartupToken;
};

#endif
