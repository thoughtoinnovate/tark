FROM alpine:3.19

# Install git, neovim, and openssh
RUN apk add --no-cache \
    git \
    neovim \
    openssh

# Configure SSH
RUN ssh-keygen -A && \
    echo "PermitRootLogin no" >> /etc/ssh/sshd_config && \
    echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config

# Set neovim as the default editor
ENV EDITOR=nvim
ENV VISUAL=nvim

# Create a non-root user with password
RUN adduser -D -s /bin/sh dev && \
    echo "dev:dev123" | chpasswd

EXPOSE 22

# Start SSH daemon
CMD ["/usr/sbin/sshd", "-D"]
