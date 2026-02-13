# syntax=docker/dockerfile:1.4
# Runtime stage - binary is pre-compiled via cross and passed via build context
FROM registry.suse.com/bci/bci-minimal:15.7

ARG TARGETARCH

# Copy the pre-compiled binary for the target architecture
COPY --from=binaries linux/${TARGETARCH}/outrider /usr/local/bin/outrider

USER 1001

ENTRYPOINT ["/usr/local/bin/outrider"]