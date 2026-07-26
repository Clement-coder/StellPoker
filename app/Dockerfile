# Build stage
FROM node:20-alpine AS builder

WORKDIR /app

ARG NEXT_PUBLIC_COORDINATOR_URL=http://localhost:8080
ARG NEXT_PUBLIC_STELLAR_EXPERT_BASE_URL=https://stellar.expert/explorer/testnet
ENV NEXT_PUBLIC_COORDINATOR_URL=${NEXT_PUBLIC_COORDINATOR_URL}
ENV NEXT_PUBLIC_STELLAR_EXPERT_BASE_URL=${NEXT_PUBLIC_STELLAR_EXPERT_BASE_URL}

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci

# Copy source code
COPY . .

# Build Next.js application
RUN npm run build

# Production stage
FROM node:20-alpine

WORKDIR /app

# Install dumb-init for proper signal handling
RUN apk add --no-cache dumb-init

# Copy package files
COPY package*.json ./

# Install production dependencies only
RUN npm ci --omit=dev

# Copy built application from builder
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/public ./public

# Create non-root user for security
RUN addgroup -g 1001 -S nodejs && \
    adduser -S nextjs -u 1001

# Use non-root user
USER nextjs

# Expose port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD node -e "require('http').get('http://localhost:3000', (r) => {if (r.statusCode !== 200) throw new Error(r.statusCode)})"

# Use dumb-init to handle signals properly
ENTRYPOINT ["dumb-init", "--"]

# Start Next.js
CMD ["npm", "start"]
